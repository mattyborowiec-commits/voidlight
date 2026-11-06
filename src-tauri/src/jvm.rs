use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JvmProfile {
    /// Zbalansowany: G1GC z tuningiem pod Minecrafta.
    Balanced,
    /// Maksymalna płynność: generacyjny ZGC (mniej mikro-przycięć).
    Smooth,
    /// Mało RAM-u: mniejszy narzut pamięci.
    Light,
}

/// Dobiera ilość RAM (MB) dla gry na podstawie całego RAM-u komputera.
/// Dla modowanej gry zwykle 4-8 GB w zupełności wystarcza; więcej szkodzi GC.
pub fn recommended_ram_mb(total_ram_mb: u64, modded: bool) -> u64 {
    let target = if modded { 6144 } else { 3072 };
    let cap = total_ram_mb / 2; // nigdy więcej niż połowa RAM-u systemu
    target.min(cap).max(2048)
}

/// Buduje argumenty JVM dla wybranego profilu.
pub fn build_args(profile: JvmProfile, ram_mb: u64) -> Vec<String> {
    let xms = match profile {
        JvmProfile::Light => ram_mb.min(2048),
        _ => ram_mb,
    };
    let mut args = vec![
        // Ignoruj flagi, których dana wersja Javy nie zna (zamiast odmówić startu).
        "-XX:+IgnoreUnrecognizedVMOptions".to_string(),
        // Sterta od razu docelowego rozmiaru: mniej zmian rozmiaru i odśmiecań podczas ładowania gry.
        // Bez AlwaysPreTouch, więc pamięć fizyczna nie jest zajmowana z góry.
        format!("-Xms{}M", xms),
        format!("-Xmx{}M", ram_mb),
        "-XX:-UsePerfData".to_string(),
        "-Dfile.encoding=UTF-8".to_string(),
    ];

    match profile {
        JvmProfile::Balanced => args.extend(
            [
                "-XX:+UseG1GC",
                "-XX:MaxGCPauseMillis=50",
                "-XX:+UnlockExperimentalVMOptions",
                "-XX:G1NewSizePercent=30",
                "-XX:G1MaxNewSizePercent=40",
                "-XX:G1HeapRegionSize=16M",
                "-XX:G1ReservePercent=20",
                "-XX:InitiatingHeapOccupancyPercent=15",
            ]
            .map(String::from),
        ),
        JvmProfile::Smooth => args.extend(
            // W nowych Javach (23+) ZGC jest domyślnie generacyjny, flaga niepotrzebna.
            ["-XX:+UseZGC"].map(String::from),
        ),
        JvmProfile::Light => args.extend(
            ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=100"].map(String::from),
        ),
    }

    args
}
