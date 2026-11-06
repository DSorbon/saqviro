//! Saqviro's tools on the command line. Tools run the way they run in the
//! app: same manifests, same checks, same job system.

use std::{
    env, fs,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
};

use saqviro_core::{
    manifest::{DataType, ParamType, Parameter, ToolManifest},
    registry::Registry,
};
use saqviro_jobs::{Config, Executor, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, Params};
use saqviro_tools::{catalog, is_available};
use saqviro_workflow::{Workflow, prepare_with};
use serde_json::{Number, Value};

const USAGE: &str = "Everyday tools. Zero uploads.

Usage:
  saqviro list [category]            List tools
  saqviro help <tool>                Show what a tool takes
  saqviro run <tool> [options] [files]
  saqviro workflow <file.json> [--out <folder>] [files]

Options of `run`:
  --<parameter> <value>              A setting of the tool
  --<parameter>                      Turn a yes/no setting on
  --no-<parameter>                   Turn it off
  --out <folder>                     Where results go
  --text <text>                      Text for a tool that takes text

Without --text, text tools read standard input.
";

fn usage_error(message: impl std::fmt::Display) -> String {
    format!("{message}\n\nTry `saqviro help`.")
}

fn list(registry: &Registry, category: Option<&str>) -> String {
    let tools = registry.tools().iter().filter(|t| is_available(t));
    let tools = tools.filter(|t| category.is_none_or(|c| t.category.as_str() == c));
    let lines: Vec<String> = tools
        .map(|t| format!("{:<28} {}", t.id.as_str(), t.description))
        .collect();
    lines.join("\n") + "\n"
}

fn describe(tool: &ToolManifest) -> String {
    let mut out = format!("{}\n{}\n", tool.name, tool.description);
    let params = tool.parameters.as_slice();
    if !params.is_empty() {
        out.push_str("\nSettings:\n");
    }
    for p in params {
        let kind = match p.param_type {
            ParamType::Enum => p.values.join(" | "),
            ParamType::Boolean => "yes or no".to_owned(),
            ParamType::Integer => "whole number".to_owned(),
            ParamType::Number => "number".to_owned(),
            ParamType::String => "text".to_owned(),
        };
        let default = p
            .default
            .as_ref()
            .map(|d| format!(" (default: {d})"))
            .unwrap_or_default();
        out.push_str(&format!("  --{:<20} {kind}{default}\n", p.id.replace('_', "-")));
    }
    out
}

fn value_of(p: &Parameter, text: &str) -> Result<Value, String> {
    let wrong = || format!("--{}: `{text}` is not a {:?}", p.id.replace('_', "-"), p.param_type).to_lowercase();
    match p.param_type {
        ParamType::Integer => text.parse::<i64>().map(Value::from).map_err(|_| wrong()),
        ParamType::Number => text
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .ok_or_else(wrong),
        ParamType::Boolean => text.parse::<bool>().map(Value::Bool).map_err(|_| wrong()),
        ParamType::String | ParamType::Enum => Ok(Value::String(text.to_owned())),
    }
}

/// What the words after `run <tool>` ask for.
#[derive(Debug, Default, PartialEq)]
struct Request {
    params: Params,
    files: Vec<PathBuf>,
    out: Option<PathBuf>,
    text: Option<String>,
}

fn parse(tool: Option<&ToolManifest>, args: &[String]) -> Result<Request, String> {
    let mut request = Request::default();
    let mut words = args.iter();
    let find = |name: &str| {
        let id = name.replace('-', "_");
        tool.and_then(|t| t.parameters.iter().find(|p| p.id == id))
    };
    while let Some(word) = words.next() {
        let Some(name) = word.strip_prefix("--") else {
            request.files.push(PathBuf::from(word));
            continue;
        };
        let (name, inline) = name.split_once('=').map_or((name, None), |(n, v)| (n, Some(v)));
        let mut next = |what: &str| {
            inline
                .map(str::to_owned)
                .or_else(|| words.next().cloned())
                .ok_or_else(|| usage_error(format!("--{what} needs a value")))
        };
        match (name, find(name), name.strip_prefix("no-").and_then(find)) {
            ("out", _, _) => request.out = Some(PathBuf::from(next("out")?)),
            ("text", None, _) => request.text = Some(next("text")?),
            (_, Some(p), _) if p.param_type == ParamType::Boolean && inline.is_none() => {
                request.params.insert(p.id.clone(), Value::Bool(true));
            }
            (_, Some(p), _) => {
                let text = next(name)?;
                request.params.insert(p.id.clone(), value_of(p, &text)?);
            }
            (_, None, Some(p)) if p.param_type == ParamType::Boolean => {
                request.params.insert(p.id.clone(), Value::Bool(false));
            }
            _ => return Err(usage_error(format!("unknown option --{name}"))),
        }
    }
    Ok(request)
}

fn stdin_text() -> Result<String, String> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text).map_err(|e| e.to_string())?;
    Ok(text)
}

fn input_for(takes: Option<DataType>, request: Request) -> Result<(JobInput, Option<PathBuf>), String> {
    let wants_text = matches!(takes, Some(DataType::Text | DataType::Json | DataType::None));
    if !wants_text {
        if request.files.is_empty() {
            return Err(usage_error("give at least one file"));
        }
        return Ok((JobInput::Files(request.files), request.out));
    }
    let text = match request.text {
        Some(text) => text,
        None if takes == Some(DataType::None) || io::stdin().is_terminal() => String::new(),
        None => stdin_text()?,
    };
    Ok((JobInput::Text(text), request.out))
}

/// Runs a job and shows its progress on stderr, leaving stdout for results.
fn execute(
    executor: Arc<dyn Executor>,
    id: &str,
    input: JobInput,
    params: Params,
    out: Option<PathBuf>,
) -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let config = Config {
        temp_root: temp.path().join("jobs"),
        workers: 1,
    };
    let show = io::stderr().is_terminal();
    let jobs = JobManager::new(config, move |s: &JobSnapshot| {
        if show && s.status == JobStatus::Running && s.progress > 0.0 {
            eprint!("\r{:>3.0}%", s.progress * 100.0);
        }
    })
    .map_err(|e| e.to_string())?;
    let done = jobs
        .wait(jobs.submit_to(id, executor, input, params, out))
        .ok_or("the job was lost")?;
    if show {
        eprint!("\r    \r");
    }
    if let Some(error) = done.error {
        return Err(error.message);
    }
    print_output(done.output, id)
}

fn print_output(output: Option<JobOutput>, id: &str) -> Result<(), String> {
    use saqviro_jobs::OutputImage;
    let mut stdout = io::stdout();
    let wrote = match output {
        Some(JobOutput::Text(text)) => writeln!(stdout, "{}", text.trim_end_matches('\n')),
        Some(JobOutput::Files(files)) => files.iter().try_for_each(|f| {
            writeln!(stdout, "{}", f.path.display())?;
            match &f.note {
                Some(note) => writeln!(io::stderr(), "{note}"),
                None => Ok(()),
            }
        }),
        Some(JobOutput::Image(OutputImage { ext, data })) => {
            let name = format!("{}.{ext}", id.replace('.', "-"));
            let bytes = decode(&data)?;
            fs::write(&name, bytes).map_err(|e| e.to_string())?;
            writeln!(stdout, "{name}")
        }
        None => Ok(()),
    };
    wrote.map_err(|e| e.to_string())
}

fn decode(data: &str) -> Result<Vec<u8>, String> {
    saqviro_tools::developer::base64::decode_bytes(data)
}

fn run_tool(registry: &Registry, args: &[String]) -> Result<(), String> {
    let (id, rest) = args.split_first().ok_or_else(|| usage_error("which tool?"))?;
    let tool = registry
        .get(id)
        .filter(|t| is_available(t))
        .ok_or_else(|| usage_error(format!("there is no tool `{id}`")))?;
    let request = parse(Some(tool), rest)?;
    let params = tool.resolve_params(&request.params).map_err(|issues| {
        let text: Vec<String> = issues.iter().map(ToString::to_string).collect();
        text.join("; ")
    })?;
    let takes = tool.inputs.first().map(|i| i.data_type);
    let (input, out) = input_for(takes, request)?;
    let executor = saqviro_tools::executor(tool).ok_or("this tool is not available")?;
    execute(executor, id, input, params, out)
}

fn run_workflow(registry: &Registry, args: &[String]) -> Result<(), String> {
    let (path, rest) = args.split_first().ok_or_else(|| usage_error("which workflow file?"))?;
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let workflow: Workflow = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let executor = prepare_with(registry, &workflow.steps, is_available, workflow.keep_going).map_err(|issues| {
        let text: Vec<String> = issues.iter().map(ToString::to_string).collect();
        text.join("\n")
    })?;
    let first = workflow.steps.first().and_then(|s| registry.get(&s.tool));
    let takes = first.and_then(|t| t.inputs.first()).map(|i| i.data_type);
    let (input, out) = input_for(takes, parse(None, rest)?)?;
    execute(executor, "workflow", input, Params::new(), out)
}

fn run(args: &[String]) -> Result<(), String> {
    let registry = catalog::registry();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["help" | "--help" | "-h"] => print!("{USAGE}"),
        ["--version" | "-V"] => println!("saqviro {}", env!("CARGO_PKG_VERSION")),
        ["list"] => print!("{}", list(&registry, None)),
        ["list", category] => print!("{}", list(&registry, Some(category))),
        ["help", id] => {
            let tool = registry
                .get(id)
                .ok_or_else(|| usage_error(format!("there is no tool `{id}`")))?;
            print!("{}", describe(tool));
        }
        ["run", ..] => run_tool(&registry, &args[1..])?,
        ["workflow", ..] => run_workflow(&registry, &args[1..])?,
        [word, ..] => return Err(usage_error(format!("unknown command `{word}`"))),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
