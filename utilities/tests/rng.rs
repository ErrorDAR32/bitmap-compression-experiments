
/// Streams of one seed are apart: no draw of one turns up early in
/// another, as it would were a stream the seed moved along.
#[test]
fn streams_of_a_seed_share_no_draws() {
    let mut seen = std::collections::HashSet::new();
    for stream in 0..64u64 {
        let mut random = utilities::rng::Rng::for_stream(42, stream);
        for _ in 0..1_000 {
            assert!(seen.insert(random.draw()), "a draw of stream {stream} seen before");
        }
    }
}
