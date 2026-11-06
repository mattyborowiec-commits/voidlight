//! Funkcje pomocnicze bez zależności zewnętrznych (łatwe do przetestowania).

use std::collections::HashMap;
use std::path::{Component, Path};

/// "grupa:artefakt:wersja[:klasyfikator]" -> ścieżka względna w repozytorium Mavena.
pub fn maven_path(name: &str) -> Option<String> {
    let mut parts = name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version_raw = parts.next()?;
    let classifier = parts.next();
    let (version, ext) = match version_raw.split_once('@') {
        Some((v, e)) => (v, e),
        None => (version_raw, "jar"),
    };
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{}/{}/{}/{}", group.replace('.', "/"), artifact, version, file))
}

/// Klucz do usuwania duplikatów bibliotek: "grupa:artefakt[:klasyfikator]" (bez wersji).
pub fn lib_key(name: &str) -> String {
    let mut p = name.split(':');
    let group = p.next().unwrap_or("");
    let artifact = p.next().unwrap_or("");
    let _version = p.next();
    match p.next() {
        Some(c) => format!("{group}:{artifact}:{c}"),
        None => format!("{group}:{artifact}"),
    }
}

/// Podstawia `${nazwa}` wartościami z mapy. Nieznane zmienne zostają bez zmian.
pub fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Czy ścieżka względna z danych zdalnych jest bezpieczna (bez "..", bez ścieżek bezwzględnych).
pub fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Czy napis nadaje się do użycia jako element ścieżki lub adresu (wersja, id, nazwa komponentu).
pub fn safe_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// Nick w Minecrafcie: 3-16 znaków, litery, cyfry i podkreślnik.
pub fn valid_player_name(s: &str) -> bool {
    (3..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64u32)
        .map(|i| (f64::from(i + 1).sin().abs() * 4294967296.0) as u32)
        .collect();

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    let mut msg = input.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&(input.len() as u64).wrapping_mul(8).to_le_bytes());

    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64usize {
            let mut f: u32;
            let g: usize;
            match i / 16 {
                0 => {
                    f = (b & c) | (!b & d);
                    g = i;
                }
                1 => {
                    f = (d & b) | (!d & c);
                    g = (5 * i + 1) % 16;
                }
                2 => {
                    f = b ^ c ^ d;
                    g = (3 * i + 5) % 16;
                }
                _ => {
                    f = c ^ (b | !d);
                    g = (7 * i) % 16;
                }
            }
            f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

/// UUID gracza offline, liczony tak samo jak w grze (UUID v3 z "OfflinePlayer:<nick>").
pub fn offline_uuid(name: &str) -> String {
    let mut h = md5(format!("OfflinePlayer:{name}").as_bytes());
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: [u8; 16]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn md5_known_vectors() {
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(
            hex(md5(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn offline_uuid_shape() {
        let u = offline_uuid("Player");
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "3");
        assert!(matches!(&u[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(u, offline_uuid("Player"));
        assert_ne!(u, offline_uuid("Other"));
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.9").unwrap(),
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert!(maven_path("zlenazwa").is_none());
    }

    #[test]
    fn lib_keys_ignore_version() {
        assert_eq!(lib_key("org.ow2.asm:asm:9.6"), lib_key("org.ow2.asm:asm:9.7"));
        assert_ne!(
            lib_key("org.lwjgl:lwjgl:3.3.3"),
            lib_key("org.lwjgl:lwjgl:3.3.3:natives-windows")
        );
    }

    #[test]
    fn substitution() {
        let mut vars: HashMap<&str, String> = HashMap::new();
        vars.insert("a", "1".to_string());
        vars.insert("b", "dwa".to_string());
        assert_eq!(substitute("x${a}y${b}z", &vars), "x1ydwaz");
        assert_eq!(substitute("${nieznana}", &vars), "${nieznana}");
        assert_eq!(substitute("brak ${a", &vars), "brak ${a");
        assert_eq!(substitute("-Djava.library.path=${a}", &vars), "-Djava.library.path=1");
    }

    #[test]
    fn path_safety() {
        assert!(safe_rel("com/mojang/x.jar"));
        assert!(!safe_rel("../x"));
        assert!(!safe_rel("a/../../x"));
        assert!(!safe_rel("/etc/passwd"));
        assert!(!safe_rel(""));
        assert!(safe_token("1.21.4"));
        assert!(safe_token("fabric-loader-0.16.9-1.21.4"));
        assert!(!safe_token(".."));
        assert!(!safe_token("a/b"));
        assert!(!safe_token(""));
    }

    #[test]
    fn player_names() {
        assert!(valid_player_name("Steve_01"));
        assert!(!valid_player_name("ab"));
        assert!(!valid_player_name("ma spacje"));
        assert!(!valid_player_name("zbyt_dlugi_nick_123456"));
    }
}
