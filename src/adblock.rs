//! Блокировка рекламы по списку доменов.

use std::{collections::HashSet, sync::OnceLock};

static LIST: &str = include_str!("blocklist.txt");

fn domains() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        LIST.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    })
}

/// Достаёт имя хоста из http(s)/ws(s)-адреса.
pub fn host_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https" | "ws" | "wss") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if host.starts_with('[') {
        return None; // IPv6-адреса в списке не встречаются
    }
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// true, если хост адреса или любой его родительский домен есть в списке.
pub fn is_blocked(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let mut candidate = host.as_str();
    loop {
        if domains().contains(candidate) {
            return true;
        }
        match candidate.split_once('.') {
            Some((_, parent)) => candidate = parent,
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_host() {
        assert_eq!(host_of("https://example.com/a?b#c"), Some("example.com"));
        assert_eq!(host_of("http://user:pw@example.com:8080/"), Some("example.com"));
        assert_eq!(host_of("wss://a.b.c"), Some("a.b.c"));
        assert_eq!(host_of("data:text/html,hi"), None);
        assert_eq!(host_of("file:///C:/x.html"), None);
    }

    #[test]
    fn blocks_listed_domains_and_subdomains() {
        assert!(is_blocked("https://doubleclick.net/ad.js"));
        assert!(is_blocked("https://stats.g.doubleclick.net/collect"));
        assert!(is_blocked("https://MC.Yandex.ru/watch/1"));
    }

    #[test]
    fn keeps_ordinary_sites() {
        assert!(!is_blocked("https://example.com/doubleclick.net"));
        assert!(!is_blocked("https://notdoubleclick.net/"));
        assert!(!is_blocked("https://yandex.ru/"));
        assert!(!is_blocked("https://net/"));
    }
}
