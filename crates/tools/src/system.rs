//! System tools. What they show stays in the app.

use std::{env, fmt::Write};

use saqviro_jobs::Params;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

use crate::{files::scan::format_size, params};

pub fn info(_: &str, _: &Params) -> Result<String, String> {
    let wanted = RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::nothing())
        .with_memory(MemoryRefreshKind::nothing().with_ram().with_swap());
    let system = System::new_with_specifics(wanted);
    let unknown = || "unknown".to_owned();
    let mut out = format!(
        "System: {} {}\nKernel: {}\nArchitecture: {}\nComputer name: {}\n",
        System::name().unwrap_or_else(unknown),
        System::os_version().unwrap_or_default(),
        System::kernel_version().unwrap_or_else(unknown),
        System::cpu_arch(),
        System::host_name().unwrap_or_else(unknown),
    );
    if let Some(cpu) = system.cpus().first() {
        let _ = writeln!(
            out,
            "\nProcessor: {}\nCores: {}",
            cpu.brand().trim(),
            system.cpus().len()
        );
    }
    let _ = writeln!(
        out,
        "\nMemory: {} of {} in use",
        format_size(system.used_memory()),
        format_size(system.total_memory())
    );
    let disks = Disks::new_with_refreshed_list();
    if !disks.list().is_empty() {
        out.push_str("\nDisks\n");
    }
    for disk in disks.list() {
        let _ = writeln!(
            out,
            "{}: {} free of {}",
            disk.mount_point().display(),
            format_size(disk.available_space()),
            format_size(disk.total_space())
        );
    }
    Ok(out)
}

pub fn environment(_: &str, params: &Params) -> Result<String, String> {
    if params::flag(params, "only_path") {
        let path = env::var_os("PATH").unwrap_or_default();
        let dirs: Vec<String> = env::split_paths(&path).map(|p| p.display().to_string()).collect();
        return Ok(dirs.join("\n"));
    }
    let mut vars: Vec<String> = env::vars_os()
        .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
        .collect();
    vars.sort();
    Ok(vars.join("\n"))
}
