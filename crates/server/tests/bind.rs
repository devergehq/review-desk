//! Where the server listens, and where it does not.
//!
//! The contract's whole authorisation story is the bind: there is no authentication, and
//! reachability is the authorisation. So "it binds 127.0.0.1" is not a comment, it is a
//! test — and one that checks more than the address the server reports, because a server
//! can report one address and be reachable at another.

mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

#[test]
fn the_listener_is_bound_to_loopback() {
    let harness = common::start_with_no_github();
    let address = harness.running.address();

    assert_eq!(
        address.ip(),
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        "the server must bind 127.0.0.1 and nothing else"
    );
}

#[test]
fn nothing_answers_on_a_non_loopback_address() {
    let harness = common::start_with_no_github();
    let port = harness.running.address().port();

    // The address the server reports is not proof on its own: a listener on 0.0.0.0
    // would report 0.0.0.0 but would also answer here. Every non-loopback address this
    // machine has is tried on the same port, and each one has to refuse.
    for address in local_addresses() {
        let target = SocketAddr::new(address, port);
        let reached = TcpStream::connect_timeout(&target, Duration::from_millis(400));
        assert!(
            reached.is_err(),
            "the server answered on {target}, which is not the loopback interface"
        );
    }
}

#[test]
fn the_server_answers_its_health_endpoint() {
    let harness = common::start_with_no_github();
    let answer = harness.api("GET", "/health", None);

    assert_eq!(answer.status, 200);
    let document = answer.json();
    assert_eq!(document["status"], "ok");
    assert!(
        document["schema_version"].as_i64().unwrap_or(0) >= 1,
        "health should prove the database is open and migrated, not only that the \
         process is running: {document}"
    );
}

#[test]
fn port_zero_binds_a_free_port_and_reports_it() {
    let harness = common::start_with_no_github();
    let port = harness.running.address().port();

    assert_ne!(port, 0, "port 0 means the bound port was never resolved");
    assert_eq!(
        harness.running.base(),
        format!("http://127.0.0.1:{port}"),
        "the reported base URL has to be the port actually bound"
    );
}

/// Every address this machine has that is not the loopback interface.
fn local_addresses() -> Vec<IpAddr> {
    // Resolved by asking the operating system for the host's own addresses. A machine
    // with only a loopback interface yields none, and the test is then vacuously true —
    // which is correct: there is no other address for the server to have been reachable
    // on.
    let mut found = Vec::new();

    if let Ok(output) = std::process::Command::new("hostname").arg("-I").output() {
        found.extend(parse_addresses(&String::from_utf8_lossy(&output.stdout)));
    }

    // macOS has no `hostname -I`.
    if found.is_empty() {
        if let Ok(output) = std::process::Command::new("ifconfig").output() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("inet ") {
                    found.extend(parse_addresses(
                        rest.split_whitespace().next().unwrap_or(""),
                    ));
                }
            }
        }
    }

    found.retain(|address| !address.is_loopback());
    found
}

fn parse_addresses(text: &str) -> Vec<IpAddr> {
    text.split_whitespace()
        .filter_map(|word| word.parse::<IpAddr>().ok())
        .collect()
}
