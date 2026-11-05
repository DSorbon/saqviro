//! Page lists as people type them: `1, 3-5, 9-7`.

use saqviro_jobs::{ErrorKind, JobError};

fn invalid(message: impl Into<String>) -> JobError {
    JobError::new(ErrorKind::InvalidInput, message)
}

fn page(text: &str, count: u32) -> Result<u32, JobError> {
    let text = text.trim();
    let number: u32 = text
        .parse()
        .map_err(|_| invalid(format!("`{text}` is not a page number, try 1, 3-5")))?;
    if number == 0 || number > count {
        let pages = if count == 1 {
            "1 page"
        } else {
            &format!("{count} pages")
        };
        return Err(invalid(format!("there is no page {number}, this PDF has {pages}")));
    }
    Ok(number)
}

/// Pages in the order listed. A range can run backwards.
pub fn parse_pages(text: &str, count: u32) -> Result<Vec<u32>, JobError> {
    let mut pages = Vec::new();
    for part in text.split(',') {
        match part.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (page(from, count)?, page(to, count)?);
                if from <= to {
                    pages.extend(from..=to);
                } else {
                    pages.extend((to..=from).rev());
                }
            }
            None => pages.push(page(part, count)?),
        }
    }
    Ok(pages)
}

/// The pages left after removing `removed`.
pub fn without(count: u32, removed: &[u32]) -> Vec<u32> {
    (1..=count).filter(|p| !removed.contains(p)).collect()
}

/// `first`, then the other pages in their usual order.
pub fn reordered(count: u32, first: &[u32]) -> Result<Vec<u32>, JobError> {
    for (i, page) in first.iter().enumerate() {
        if first[..i].contains(page) {
            return Err(invalid(format!("page {page} is listed twice")));
        }
    }
    let mut pages = first.to_vec();
    pages.extend(without(count, first));
    Ok(pages)
}

/// The list in qpdf's page range syntax.
pub fn qpdf_range(pages: &[u32]) -> String {
    pages.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}
