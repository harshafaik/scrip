use rand::Rng;

/// Build a valid IPv4 address from an optional numeric prefix.
///
/// Numeric leading octets are kept; any missing octets are filled with a random
/// host value. CIDR suffixes (`/16`) and placeholder octets (`x`) are ignored.
pub(crate) fn ipv4(prefix: &str, rng: &mut impl Rng) -> String {
    let mut octets: Vec<u8> = prefix
        .trim()
        .split('/')
        .next()
        .unwrap_or("")
        .split('.')
        .filter_map(|part| part.parse::<u8>().ok())
        .take(4)
        .collect();

    while octets.len() < 4 {
        octets.push(rng.random_range(1..=254));
    }

    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}
