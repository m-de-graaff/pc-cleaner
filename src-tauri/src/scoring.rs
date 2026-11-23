use crate::model::Risk;

pub fn score(risk: Risk, size_bytes: u64) -> u8 {
    let base = match risk {
        Risk::Safe => 70.0,
        Risk::Review => 40.0,
        Risk::Caution => 10.0,
    };
    let mib = size_bytes as f64 / (1024.0 * 1024.0);
    // 10 MiB -> +10, 100 MiB -> +20, 1 GiB+ -> +30
    let bonus = if mib < 1.0 { 0.0 } else { (mib.log10() * 10.0).min(30.0) };
    (base + bonus).round() as u8
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    const MIB: u64 = 1024 * 1024;

    #[test]
    fn base_score_follows_risk() {
        assert_eq!(score(Risk::Safe, 0), 70);
        assert_eq!(score(Risk::Review, 0), 40);
        assert_eq!(score(Risk::Caution, 0), 10);
    }

    #[test]
    fn size_adds_up_to_thirty() {
        assert_eq!(score(Risk::Review, 10 * MIB), 50);
        assert_eq!(score(Risk::Safe, 1024 * MIB), 100);
        assert_eq!(score(Risk::Safe, 100_000 * MIB), 100);
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * MIB), "1.0 GB");
    }
}
