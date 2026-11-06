//! Tools that read keys, certificates and passwords. Nothing leaves the
//! device, and nothing given to them is kept.

use std::fmt::Write;

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use saqviro_jobs::Params;
use sha2::{Digest, Sha256};
use x509_parser::prelude::*;

pub fn password_strength(text: &str, _: &Params) -> Result<String, String> {
    let password = text.trim_end_matches(['\r', '\n']);
    if password.is_empty() {
        return Err("enter a password".into());
    }
    let result = zxcvbn::zxcvbn(password, &[]);
    let score = match u8::from(result.score()) {
        0 => "very weak",
        1 => "weak",
        2 => "fair",
        3 => "strong",
        _ => "very strong",
    };
    let times = result.crack_times();
    let mut out = format!(
        "Strength: {score}\nGuesses needed: about 10^{:.0}\nOnline attack, limited: {}\nOffline attack, fast hash: {}",
        result.guesses_log10(),
        times.online_throttling_100_per_hour(),
        times.offline_fast_hashing_1e10_per_second(),
    );
    if let Some(feedback) = result.feedback() {
        if let Some(warning) = feedback.warning() {
            let _ = write!(out, "\n\n{warning}");
        }
        for tip in feedback.suggestions() {
            let _ = write!(out, "\n{tip}");
        }
    }
    Ok(out)
}

fn colons(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn describe(der: &[u8]) -> Result<String, String> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|e| format!("this is not a certificate: {e}"))?;
    let validity = cert.validity();
    let mut out = format!(
        "Subject: {}\nIssuer: {}\nValid from: {}\nValid until: {}\nSerial: {}\nSignature: {}\nSHA-256: {}\n",
        cert.subject(),
        cert.issuer(),
        validity.not_before,
        validity.not_after,
        cert.raw_serial_as_string(),
        cert.signature_algorithm.algorithm,
        colons(&Sha256::digest(der)),
    );
    if !validity.is_valid() {
        out.push_str("This certificate is not valid today.\n");
    }
    if let Ok(Some(names)) = cert.subject_alternative_name() {
        let shown: Vec<String> = names.value.general_names.iter().map(ToString::to_string).collect();
        if !shown.is_empty() {
            let _ = writeln!(out, "Names: {}", shown.join(", "));
        }
    }
    Ok(out)
}

pub fn certificate(text: &str, _: &Params) -> Result<String, String> {
    let mut sections = Vec::new();
    for pem in Pem::iter_from_buffer(text.as_bytes()) {
        let pem = pem.map_err(|e| format!("this PEM text is damaged: {e}"))?;
        if pem.label == "CERTIFICATE" {
            sections.push(describe(&pem.contents)?);
        }
    }
    if sections.is_empty() {
        // Base64 without the BEGIN and END lines.
        let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let der = STANDARD
            .decode(clean)
            .map_err(|_| "paste a certificate in PEM format".to_owned())?;
        sections.push(describe(&der)?);
    }
    Ok(sections.join("\n"))
}

pub fn ssh_fingerprint(text: &str, _: &Params) -> Result<String, String> {
    let mut lines = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if line.contains("PRIVATE KEY") {
            return Err("this is a private key; paste the public one (.pub)".into());
        }
        let mut parts = line.split_whitespace();
        let (Some(kind), Some(body)) = (parts.next(), parts.next()) else {
            return Err("paste a public key, like ssh-ed25519 AAAA…".into());
        };
        let key = STANDARD.decode(body).map_err(|_| "this key is damaged".to_owned())?;
        let comment = parts.collect::<Vec<_>>().join(" ");
        let hash = STANDARD_NO_PAD.encode(Sha256::digest(&key));
        lines.push(format!("SHA256:{hash} {comment} ({kind})").replace("  ", " "));
    }
    if lines.is_empty() {
        return Err("paste a public key, like ssh-ed25519 AAAA…".into());
    }
    Ok(lines.join("\n"))
}
