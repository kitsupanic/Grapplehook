//! Update check, driven through core's public interface. Version comparison is checked
//! through it too: a release is an update only if its version is newer.
//! The GitHub API is replaced by a one-shot local HTTP server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use grapplehook_lib::core::{check_for_update, UpdateInfo};

/// Serve exactly one canned HTTP response; returns the URL to hit.
fn serve_once(response: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/releases/latest", listener.local_addr().unwrap());
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

fn serve_json(body: &str) -> String {
    serve_once(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ))
}

fn none(current: &str) -> UpdateInfo {
    UpdateInfo {
        update_available: false,
        current_version: current.into(),
        latest_version: None,
    }
}

/// Is a release tagged `latest` an update for an app at version `current`?
fn is_update(latest: &str, current: &str) -> bool {
    let url = serve_json(&format!(r#"{{"tag_name":"{latest}"}}"#));
    check_for_update(current, &url).update_available
}

#[test]
fn versions_are_compared_numerically_not_lexically() {
    assert!(is_update("1.2.10", "1.2.9"));
    assert!(!is_update("1.2.9", "1.2.10"));
    assert!(is_update("10.0.0", "2.0.0"));
    assert!(!is_update("2.0.0", "10.0.0"));
}

#[test]
fn a_missing_version_segment_counts_as_zero() {
    assert!(!is_update("1.2", "1.2.0"));
    assert!(!is_update("1.2.0.0", "1.2"));
    assert!(is_update("1.2.1", "1.2"));
    assert!(is_update("1.3", "1.2.9"));
}

#[test]
fn pre_release_and_build_tags_are_ignored() {
    assert!(!is_update("1.2.3-rc.1", "1.2.3"));
    assert!(!is_update("1.2.3", "1.2.3-beta"));
    assert!(!is_update("1.2.3+build.5", "1.2.3"));
    assert!(is_update("1.2.4-rc.1", "1.2.3"));
    assert!(!is_update("1.2.3-rc.1", "1.2.4"));
}

#[test]
fn a_segment_that_is_not_a_number_counts_as_zero() {
    assert!(!is_update("abc", "0.0.0"));
    assert!(!is_update("1.x.3", "1.0.3"));
    assert!(is_update("1.x.3", "1.0.2"));
}

#[test]
fn newer_release_is_reported_with_the_leading_v_stripped() {
    let url = serve_json(r#"{"tag_name":"v1.2.10"}"#);

    let info = check_for_update("1.2.9", &url);

    assert_eq!(
        info,
        UpdateInfo {
            update_available: true,
            current_version: "1.2.9".into(),
            latest_version: Some("1.2.10".into()),
        }
    );
}

#[test]
fn same_release_is_not_an_update() {
    let url = serve_json(r#"{"tag_name":"v0.2.0"}"#);

    let info = check_for_update("0.2.0", &url);

    assert!(!info.update_available);
    assert_eq!(info.latest_version.as_deref(), Some("0.2.0"));
}

#[test]
fn rate_limited_response_counts_as_no_update() {
    let url = serve_once(
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
    );

    assert_eq!(check_for_update("0.2.0", &url), none("0.2.0"));
}

#[test]
fn invalid_json_counts_as_no_update() {
    let url = serve_json("not json");

    assert_eq!(check_for_update("0.2.0", &url), none("0.2.0"));
}

#[test]
fn missing_or_non_string_tag_counts_as_no_update() {
    let missing = serve_json(r#"{"name":"x"}"#);
    let numeric = serve_json(r#"{"tag_name":5}"#);

    assert_eq!(check_for_update("0.2.0", &missing), none("0.2.0"));
    assert_eq!(check_for_update("0.2.0", &numeric), none("0.2.0"));
}

#[test]
fn offline_counts_as_no_update() {
    // Bind then drop, so the port is closed.
    let url = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/releases/latest", listener.local_addr().unwrap())
    };

    assert_eq!(check_for_update("0.2.0", &url), none("0.2.0"));
}
