//! Lorem Ipsum. The same settings always give the same text.

use saqviro_jobs::Params;

use crate::params;

const OPENING: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit";
const WORDS: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "eu",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
];

/// A small generator: only the look of the text matters here.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below
    }
}

fn sentence(numbers: &mut Numbers) -> String {
    let length = 6 + numbers.next(10);
    let mut words: Vec<&str> = (0..length).map(|_| WORDS[numbers.next(WORDS.len())]).collect();
    words.dedup();
    let text = words.join(" ");
    let mut chars = text.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or_default();
    format!("{first}{}.", chars.as_str())
}

pub fn run(_: &str, params: &Params) -> Result<String, String> {
    let mut numbers = Numbers(1);
    let sentences = params::number(params, "sentences", 5).max(1);
    let paragraphs: Vec<String> = (0..params::number(params, "paragraphs", 3).max(1))
        .map(|p| {
            let mut made: Vec<String> = (0..sentences).map(|_| sentence(&mut numbers)).collect();
            if p == 0 {
                made[0] = format!("{OPENING}.");
            }
            made.join(" ")
        })
        .collect();
    Ok(paragraphs.join("\n\n"))
}
