//! One happy-path smoke test: a SET replicates through the leader and a GET
//! reads it back. This is intentionally the only test — a move-fast build.

use raft_kv::raft::cluster::Cluster;
use raft_kv::resp::{parse, Command};
use raft_kv::kv::KvReply;

#[test]
fn set_replicates_and_get_reads_it_back() {
    let mut cluster = Cluster::new(3, 0xC0FF_EE00);

    // Elect a leader.
    cluster.run_ticks(30);
    let leader = cluster.leader().expect("a leader should be elected");

    // SET hello world through the leader.
    let set = parse(b"*3\r\n$3\r\nSET\r\n$5\r\nhello\r\n$5\r\nworld\r\n").unwrap();
    assert_eq!(
        set,
        Command::Set { key: "hello".into(), value: "world".into() }
    );
    assert!(cluster.client_write(set), "leader should accept the write");

    // Let it replicate, commit, and apply.
    cluster.run_ticks(30);

    // GET hello back through the leader's applied state machine.
    let get = parse(b"GET hello\r\n").unwrap();
    let reply = cluster.nodes[leader as usize].read(&get);
    assert_eq!(reply, KvReply::Value(Some("world".to_string())));
}
