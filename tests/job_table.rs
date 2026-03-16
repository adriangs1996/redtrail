use redtrail::workflows::command::jobs::JobTable;

#[test]
fn add_and_list_jobs() {
    let mut table = JobTable::new();
    let id = table.add("nmap -sV 10.10.10.1".into(), 0);
    assert_eq!(id, 1);
    assert_eq!(table.list().len(), 1);
    assert_eq!(table.list()[0].command, "nmap -sV 10.10.10.1");
}

#[test]
fn finish_job() {
    let mut table = JobTable::new();
    let id = table.add("gobuster".into(), 0);
    table.finish(id, 0);
    let job = table.get(id).unwrap();
    assert!(job.finished);
    assert_eq!(job.exit_code, Some(0));
}

#[test]
fn next_id_increments() {
    let mut table = JobTable::new();
    let id1 = table.add("a".into(), 0);
    let id2 = table.add("b".into(), 1);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn running_jobs_count() {
    let mut table = JobTable::new();
    table.add("a".into(), 0);
    table.add("b".into(), 1);
    table.finish(1, 0);
    assert_eq!(table.running_count(), 1);
}
