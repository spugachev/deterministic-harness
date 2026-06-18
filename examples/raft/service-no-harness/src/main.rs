//! Demo entrypoint: spin up a 3-node in-memory cluster, elect a leader, write
//! a key through the leader, and read it back once it has replicated + applied.

use raft_kv::raft::cluster::Cluster;
use raft_kv::resp::{parse, Command};

fn main() {
    let mut cluster = Cluster::new(3, 0xDEAD_BEEF);

    // Let the cluster settle and elect a leader.
    cluster.run_ticks(30);
    let leader = match cluster.leader() {
        Some(id) => id,
        None => {
            eprintln!("no leader elected");
            std::process::exit(1);
        }
    };
    println!("leader elected: node {leader}");

    // Parse a SET off the wire and submit it to the leader.
    let set = parse(b"*3\r\n$3\r\nSET\r\n$5\r\nhello\r\n$5\r\nworld\r\n")
        .expect("valid SET frame");
    assert_eq!(
        set,
        Command::Set {
            key: "hello".into(),
            value: "world".into()
        }
    );
    let accepted = cluster.client_write(set);
    println!("write accepted by leader: {accepted}");

    // Drive ticks so the entry replicates to a majority, commits, and applies.
    cluster.run_ticks(30);

    // Read it back through the leader's applied state machine.
    let get = parse(b"GET hello\r\n").expect("valid GET frame");
    let reply = cluster.nodes[leader as usize].read(&get);
    println!("GET hello -> {reply:?}");
}
