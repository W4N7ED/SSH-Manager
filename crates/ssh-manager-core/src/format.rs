//! Formatage lisible des durées, tailles et instants.

/// « il y a 2 heures », « hier », « il y a 3 semaines »… à partir de deux
/// horodatages en millisecondes.
pub fn last_used(timestamp: Option<i64>, now: i64) -> String {
    let Some(timestamp) = timestamp else {
        return "jamais".into();
    };
    let minutes = ((now - timestamp) as f64 / 60_000.0).round() as i64;
    if minutes < 1 {
        return "à l'instant".into();
    }
    if minutes < 60 {
        return ago(minutes, "minute", "minutes");
    }
    let hours = (minutes as f64 / 60.0).round() as i64;
    if hours < 24 {
        return ago(hours, "heure", "heures");
    }
    let days = (hours as f64 / 24.0).round() as i64;
    match days {
        1 => "hier".into(),
        2 => "avant-hier".into(),
        3..7 => ago(days, "jour", "jours"),
        _ if days < 35 => ago((days as f64 / 7.0).round() as i64, "semaine", "semaines"),
        _ => ago((days as f64 / 30.0).round() as i64, "mois", "mois"),
    }
}

fn ago(value: i64, singular: &str, plural: &str) -> String {
    format!(
        "il y a {value} {}",
        if value > 1 { plural } else { singular }
    )
}

/// Taille en octets, en unités binaires : « 812.0 MB », « 4.20 GB ».
pub fn file_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KIB {
        format!("{bytes} B")
    } else if value < KIB * KIB {
        format!("{:.1} KB", value / KIB)
    } else if value < KIB * KIB * KIB {
        format!("{:.1} MB", value / KIB / KIB)
    } else {
        format!("{:.2} GB", value / KIB / KIB / KIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;

    #[test]
    fn should_say_never_without_timestamp() {
        assert_eq!(last_used(None, 0), "jamais");
    }

    #[test]
    fn should_say_just_now_under_a_minute() {
        assert_eq!(last_used(Some(0), 20_000), "à l'instant");
    }

    #[test]
    fn should_use_singular_for_one_unit() {
        assert_eq!(last_used(Some(0), HOUR), "il y a 1 heure");
    }

    #[test]
    fn should_use_plural_above_one_unit() {
        assert_eq!(last_used(Some(0), 5 * HOUR), "il y a 5 heures");
    }

    #[test]
    fn should_say_yesterday_after_one_day() {
        assert_eq!(last_used(Some(0), DAY), "hier");
    }

    #[test]
    fn should_switch_to_weeks_after_a_week() {
        assert_eq!(last_used(Some(0), 14 * DAY), "il y a 2 semaines");
    }

    #[test]
    fn should_format_sizes_in_binary_units() {
        assert_eq!(file_size(512), "512 B");
        assert_eq!(file_size(2150), "2.1 KB");
        assert_eq!(file_size(4_509_715_660), "4.20 GB");
    }
}
