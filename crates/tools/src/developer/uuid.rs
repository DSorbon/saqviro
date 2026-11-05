//! UUID Generator.

use saqviro_jobs::Params;
use uuid::Uuid;

use crate::params;

pub fn run(_: &str, params: &Params) -> Result<String, String> {
    let make: fn() -> Uuid = match params::text(params, "version") {
        "7" => Uuid::now_v7,
        _ => Uuid::new_v4,
    };
    let count = params::number(params, "count", 1).clamp(1, 1000);
    let ids: Vec<String> = (0..count).map(|_| make().to_string()).collect();
    let ids = ids.join("\n");
    Ok(if params::flag(params, "uppercase") {
        ids.to_uppercase()
    } else {
        ids
    })
}
