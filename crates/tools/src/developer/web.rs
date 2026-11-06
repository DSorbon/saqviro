//! URLs, HTML and HTTP.

use std::fmt::Write;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use saqviro_jobs::Params;
use url::Url;

use crate::params;

/// What `encodeURIComponent` leaves alone.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');
/// What `encodeURI` leaves alone: a whole URL stays a URL.
const WHOLE_URL: &AsciiSet = &COMPONENT
    .remove(b':')
    .remove(b'/')
    .remove(b'?')
    .remove(b'#')
    .remove(b'[')
    .remove(b']')
    .remove(b'@')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=');

pub fn url_encode(text: &str, params: &Params) -> Result<String, String> {
    if params::text(params, "mode") == "decode" {
        let decoded = percent_decode_str(text.trim()).decode_utf8();
        return decoded
            .map(|t| t.into_owned())
            .map_err(|_| "this does not decode to text".into());
    }
    let set = if params::flag(params, "keep_url_characters") {
        WHOLE_URL
    } else {
        COMPONENT
    };
    Ok(utf8_percent_encode(text, set).to_string())
}

pub fn url_parse(text: &str, _: &Params) -> Result<String, String> {
    let url = Url::parse(text.trim()).map_err(|e| format!("this is not a full URL: {e}"))?;
    let mut out = format!("Scheme: {}\n", url.scheme());
    if !url.username().is_empty() {
        let _ = writeln!(out, "User: {}", url.username());
    }
    if let Some(host) = url.host_str() {
        let _ = writeln!(out, "Host: {host}");
    }
    if let Some(port) = url.port_or_known_default() {
        let _ = writeln!(out, "Port: {port}");
    }
    let _ = writeln!(out, "Path: {}", url.path());
    if let Some(fragment) = url.fragment() {
        let _ = writeln!(out, "Fragment: {fragment}");
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if !pairs.is_empty() {
        out.push_str("\nQuery\n");
        for (key, value) in pairs {
            let _ = writeln!(out, "{key} = {value}");
        }
    }
    Ok(out)
}

pub fn html_entities(text: &str, params: &Params) -> Result<String, String> {
    Ok(if params::text(params, "mode") == "decode" {
        html_escape::decode_html_entities(text).into_owned()
    } else {
        html_escape::encode_quoted_attribute(text).into_owned()
    })
}

const STATUSES: &[(u16, &str, &str)] = &[
    (100, "Continue", "Go on sending the request."),
    (
        101,
        "Switching Protocols",
        "The server is changing protocol, as to WebSocket.",
    ),
    (
        102,
        "Processing",
        "The request was received and is still being worked on.",
    ),
    (
        103,
        "Early Hints",
        "Headers to start preloading with, before the real response.",
    ),
    (200, "OK", "The request succeeded."),
    (201, "Created", "The request succeeded and made something new."),
    (202, "Accepted", "The request was taken, but is not finished."),
    (203, "Non-Authoritative Information", "A proxy changed the response."),
    (
        204,
        "No Content",
        "The request succeeded and there is nothing to return.",
    ),
    (205, "Reset Content", "The request succeeded; clear the form or view."),
    (206, "Partial Content", "Part of the resource, for a Range request."),
    (207, "Multi-Status", "Several results for several operations (WebDAV)."),
    (
        208,
        "Already Reported",
        "Members already listed in this response (WebDAV).",
    ),
    (226, "IM Used", "The response is the result of instance manipulations."),
    (300, "Multiple Choices", "There is more than one possible response."),
    (301, "Moved Permanently", "The resource has a new permanent URL."),
    (302, "Found", "The resource is at another URL for now."),
    (303, "See Other", "Fetch the result from another URL with GET."),
    (304, "Not Modified", "The cached copy is still good."),
    (
        307,
        "Temporary Redirect",
        "Repeat the request at another URL, with the same method.",
    ),
    (308, "Permanent Redirect", "Moved for good; keep the same method."),
    (400, "Bad Request", "The server cannot understand the request."),
    (401, "Unauthorized", "Authentication is needed."),
    (
        402,
        "Payment Required",
        "Reserved; some APIs use it for billing limits.",
    ),
    (403, "Forbidden", "The server understood, and refuses."),
    (404, "Not Found", "Nothing exists at this URL."),
    (405, "Method Not Allowed", "This URL does not support this method."),
    (
        406,
        "Not Acceptable",
        "No version of the resource fits the Accept headers.",
    ),
    (
        407,
        "Proxy Authentication Required",
        "Authenticate with the proxy first.",
    ),
    (408, "Request Timeout", "The client took too long to send the request."),
    (409, "Conflict", "The request clashes with the current state."),
    (410, "Gone", "The resource was removed for good."),
    (411, "Length Required", "The request needs a Content-Length header."),
    (412, "Precondition Failed", "A condition in the headers is not met."),
    (413, "Content Too Large", "The request body is too big."),
    (414, "URI Too Long", "The URL is too long."),
    (415, "Unsupported Media Type", "The server does not accept this format."),
    (416, "Range Not Satisfiable", "The requested range does not exist."),
    (417, "Expectation Failed", "The Expect header cannot be met."),
    (418, "I'm a Teapot", "An April Fools' joke that stayed."),
    (421, "Misdirected Request", "This server cannot answer for this host."),
    (
        422,
        "Unprocessable Content",
        "The request is well formed, but its content is wrong.",
    ),
    (423, "Locked", "The resource is locked (WebDAV)."),
    (424, "Failed Dependency", "An earlier request failed (WebDAV)."),
    (425, "Too Early", "The server will not risk a replayed request."),
    (426, "Upgrade Required", "Switch to another protocol."),
    (428, "Precondition Required", "The request must be conditional."),
    (429, "Too Many Requests", "Rate limit reached; slow down."),
    (431, "Request Header Fields Too Large", "The headers are too big."),
    (451, "Unavailable For Legal Reasons", "Blocked for legal reasons."),
    (500, "Internal Server Error", "Something broke on the server."),
    (501, "Not Implemented", "The server does not support this."),
    (502, "Bad Gateway", "A server behind the gateway answered badly."),
    (
        503,
        "Service Unavailable",
        "The server is overloaded or down for maintenance.",
    ),
    (
        504,
        "Gateway Timeout",
        "A server behind the gateway did not answer in time.",
    ),
    (
        505,
        "HTTP Version Not Supported",
        "The server does not support this HTTP version.",
    ),
    (
        506,
        "Variant Also Negotiates",
        "The server's content negotiation is misconfigured.",
    ),
    (507, "Insufficient Storage", "The server has run out of space (WebDAV)."),
    (508, "Loop Detected", "The server found an endless loop (WebDAV)."),
    (510, "Not Extended", "More extensions are needed for this request."),
    (
        511,
        "Network Authentication Required",
        "Sign in to the network, as on public Wi-Fi.",
    ),
];

pub fn http_status(text: &str, _: &Params) -> Result<String, String> {
    let query = text.trim().to_lowercase();
    let found: Vec<String> = STATUSES
        .iter()
        .filter(|(code, name, _)| {
            let code = code.to_string();
            let class = query.len() == 3 && query.ends_with("xx") && code.starts_with(&query[..1]);
            code == query || class || (query.parse::<u16>().is_err() && name.to_lowercase().contains(&query))
        })
        .map(|(code, name, meaning)| format!("{code} {name}\n{meaning}"))
        .collect();
    if found.is_empty() {
        return Err(format!("no status matches `{}`", text.trim()));
    }
    Ok(found.join("\n\n"))
}

pub fn mime(text: &str, _: &Params) -> Result<String, String> {
    let query = text.trim().to_lowercase();
    if query.contains('/') {
        let (kind, subtype) = query.split_once('/').unwrap_or_default();
        let extensions =
            mime_guess::get_extensions(kind, subtype).ok_or_else(|| format!("`{query}` is not a known type"))?;
        return Ok(extensions.join("\n"));
    }
    let extension = query.rsplit('.').next().unwrap_or_default();
    let types = mime_guess::from_ext(extension);
    if types.is_empty() {
        return Err(format!("`{extension}` is not a known extension"));
    }
    Ok(types.iter().map(|m| m.to_string()).collect::<Vec<_>>().join("\n"))
}
