mod disconnected;
mod subject;

fn main() {
    let _uncalled = subject::uncalled as fn(u32) -> u32;
    assert_eq!(subject::executed(5), 6);
    assert_eq!(subject::runtime_only(), 88);
}
