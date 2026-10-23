// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! The game list of the Library: filtered by what the player typed and chose, put in folders (by first letter, developer or franchise) and
//! ordered; and the search score that decides which games match a few typed words.

use crate::games::GameProfile;
use crate::prefs::{filter, Grouping};
use std::cmp::Reverse;
use unicode_normalization::UnicodeNormalization;

pub const UNKNOWN_GROUP: &str = "Other";

/// One folder of the list.
#[derive(Debug, Clone)]
pub struct GameGroup<'a> {
    pub header: String,
    pub games: Vec<&'a GameProfile>,
}

/// What the list needs to know about each game.
pub struct ListContext<'a> {
    pub on_this_pc: &'a dyn Fn(&GameProfile) -> bool,
    pub mod_installed: &'a dyn Fn(&GameProfile) -> bool,
    pub is_favorite: &'a dyn Fn(&GameProfile) -> bool,
    /// When the player last played a game (seconds since 1970), if ever.
    pub last_played: &'a dyn Fn(&GameProfile) -> Option<i64>,
}

#[allow(clippy::too_many_arguments)]
pub fn build<'a>(
    games: &[&'a GameProfile],
    query: &str,
    filter_flags: u32,
    grouping: Grouping,
    descending: bool,
    recent_first: bool,
    ctx: &ListContext,
) -> Vec<GameGroup<'a>> {
    let q = query.trim();
    let mut shown: Vec<&GameProfile> = games.iter().copied().filter(|g| matches(g, q)).collect();
    if filter_flags & filter::ON_THIS_PC != 0 {
        shown.retain(|g| (ctx.on_this_pc)(g));
    }
    if filter_flags & filter::MOD_INSTALLED != 0 {
        shown.retain(|g| (ctx.mod_installed)(g));
    }
    if filter_flags & filter::FAVORITES != 0 {
        shown.retain(|g| (ctx.is_favorite)(g));
    }
    let name = |g: &GameProfile| g.display_name.to_lowercase();
    if descending {
        shown.sort_by_key(|g| Reverse(name(g)));
    } else {
        shown.sort_by_key(|g| name(g));
    }
    // "recently played" reorders the games inside each folder (the folders keep their order); never-played games follow by title
    if recent_first {
        shown.sort_by(|a, b| {
            (ctx.last_played)(b)
                .unwrap_or(i64::MIN)
                .cmp(&(ctx.last_played)(a).unwrap_or(i64::MIN))
                .then_with(|| name(a).cmp(&name(b)))
        });
    }

    let key_of = |g: &GameProfile| -> String {
        match grouping {
            Grouping::Developer => name_or(&g.developer),
            Grouping::Franchise => name_or(&g.franchise),
            Grouping::Title => letter_of(&g.display_name),
        }
    };
    let mut groups: Vec<GameGroup> = Vec::new();
    for g in shown {
        let key = key_of(g);
        match groups.iter_mut().find(|x| x.header.eq_ignore_ascii_case(&key)) {
            Some(group) => group.games.push(g),
            None => groups.push(GameGroup {
                header: key,
                games: vec![g],
            }),
        }
    }
    // "Other" and "#" always come last, whichever way the folders are ordered
    groups.sort_by(|a, b| {
        let (ca, cb) = (is_catch_all(&a.header), is_catch_all(&b.header));
        ca.cmp(&cb).then_with(|| {
            let o = a.header.to_lowercase().cmp(&b.header.to_lowercase());
            if descending {
                o.reverse()
            } else {
                o
            }
        })
    });
    groups
}

pub fn matches(g: &GameProfile, query: &str) -> bool {
    query.trim().is_empty() || score(g, query) > 0
}

/// How well a game matches typed words (0 = it does not): every word has to find something.
pub fn score(g: &GameProfile, query: &str) -> i32 {
    let words = words(query);
    if words.is_empty() {
        return 0;
    }
    let (title, id, dev, fran) = (
        normalize(&g.display_name),
        normalize(&g.id),
        normalize(&g.developer),
        normalize(&g.franchise),
    );
    let title_words: Vec<&str> = title.split(' ').filter(|w| !w.is_empty()).collect();
    let initials: String = title_words.iter().filter_map(|w| w.chars().next()).collect();
    let compact = title.replace(' ', "");
    let mut total = 0;
    for w in &words {
        let best = if title_words.iter().any(|t| t.starts_with(w.as_str())) {
            40
        } else if title.contains(w.as_str()) {
            25
        } else if id.contains(w.as_str()) {
            22
        } else if initials.starts_with(w.as_str()) && w.chars().count() >= 2 {
            20
        } else if self::words(&dev)
            .iter()
            .chain(self::words(&fran).iter())
            .any(|t| t.starts_with(w.as_str()))
        {
            12
        } else if dev.contains(w.as_str()) || fran.contains(w.as_str()) {
            8
        } else if w.chars().count() >= 3 && is_subsequence(w, &compact) {
            4
        } else {
            0
        };
        if best == 0 {
            return 0;
        }
        total += best;
    }
    if title.starts_with(&words.join(" ")) {
        total += 30; // the title begins with what was typed
    }
    total
}

fn words(s: &str) -> Vec<String> {
    normalize(s).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// Lower case, without accents, everything that is not a letter or a digit a single space.
pub fn normalize(s: &str) -> String {
    let mapped: String = s
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect();
    mapped.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut n = needle.chars().peekable();
    for c in hay.chars() {
        if n.peek() == Some(&c) {
            n.next();
        }
    }
    n.peek().is_none()
}

fn name_or(s: &str) -> String {
    if s.trim().is_empty() {
        UNKNOWN_GROUP.to_string()
    } else {
        s.trim().to_string()
    }
}

fn is_catch_all(header: &str) -> bool {
    header == UNKNOWN_GROUP || header == "#"
}

/// The first letter of a title (upper case), or `#` for a title that starts with anything else.
pub fn letter_of(title: &str) -> String {
    match title.trim_start().chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().collect(),
        _ => "#".to_string(),
    }
}

/// The key a closed folder is remembered by.
pub fn folder_key(grouping: Grouping, header: &str) -> String {
    format!("{}:{header}", grouping.key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::GameRegistry;
    use cairn_setup::catalog::{GameList, GameManifest};
    use cairn_setup::catalog_client::{CatalogGame, CatalogSnapshot};

    fn games() -> GameRegistry {
        let list = r#"{ "schema": 1, "games": [ {"id":"kcd2"}, {"id":"noita"}, {"id":"dsp"}, {"id":"cs"}, {"id":"x"} ] }"#;
        let g = |id: &str, name: &str, dev: &str, fran: &str| {
            let list = GameList::parse(list).unwrap();
            (
                list.games.iter().find(|e| e.id == id).unwrap().clone(),
                GameManifest::parse(&format!(
                    r#"{{ "schema": 1, "id": "{id}", "name": "{name}", "developer": "{dev}", "franchise": "{fran}" }}"#
                ))
                .unwrap(),
            )
        };
        let all = [
            g("kcd2", "Kingdom Come: Deliverance II", "Warhorse Studios", "Kingdom Come"),
            g("noita", "Noita", "Nolla Games", ""),
            g("dsp", "Dyson Sphere Program", "Youthcat Studio", ""),
            g("cs", "Cities: Skylines", "Colossal Order", "Cities: Skylines"),
            g("x", "3 Dragons", "", ""),
        ];
        let mut r = GameRegistry::new();
        r.apply(&CatalogSnapshot {
            list: GameList::parse(list).unwrap(),
            games: all
                .into_iter()
                .map(|(entry, manifest)| CatalogGame {
                    entry,
                    manifest,
                    base_url: "https://x.test/".into(),
                })
                .collect(),
            from_cache: false,
        });
        r
    }

    fn ctx<'a>(
        on_pc: &'a dyn Fn(&GameProfile) -> bool,
        fav: &'a dyn Fn(&GameProfile) -> bool,
        last: &'a dyn Fn(&GameProfile) -> Option<i64>,
    ) -> ListContext<'a> {
        ListContext {
            on_this_pc: on_pc,
            mod_installed: &|_| false,
            is_favorite: fav,
            last_played: last,
        }
    }

    fn headers(g: &[GameGroup]) -> Vec<String> {
        g.iter()
            .map(|x| {
                format!(
                    "{}={}",
                    x.header,
                    x.games.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(",")
                )
            })
            .collect()
    }

    #[test]
    fn games_are_put_in_folders_by_first_letter_and_a_hash_comes_last() {
        let r = games();
        let all = r.all();
        let c = ctx(&|_| true, &|_| false, &|_| None);
        let g = build(&all, "", filter::ALL, Grouping::Title, false, false, &c);
        assert_eq!(headers(&g), vec!["C=cs", "D=dsp", "K=kcd2", "N=noita", "#=x"]);
        let g = build(&all, "", filter::ALL, Grouping::Title, true, false, &c);
        assert_eq!(headers(&g), vec!["N=noita", "K=kcd2", "D=dsp", "C=cs", "#=x"]);
    }

    #[test]
    fn folders_by_developer_or_franchise_put_the_unknown_ones_in_other() {
        let r = games();
        let all = r.all();
        let c = ctx(&|_| true, &|_| false, &|_| None);
        let by_dev = build(&all, "", filter::ALL, Grouping::Developer, false, false, &c);
        assert_eq!(
            headers(&by_dev),
            vec![
                "Colossal Order=cs",
                "Nolla Games=noita",
                "Warhorse Studios=kcd2",
                "Youthcat Studio=dsp",
                "Other=x"
            ]
        );
        let by_fran = build(&all, "", filter::ALL, Grouping::Franchise, false, false, &c);
        assert_eq!(
            headers(&by_fran),
            vec!["Cities: Skylines=cs", "Kingdom Come=kcd2", "Other=x,dsp,noita"]
        );
    }

    #[test]
    fn the_filters_narrow_the_list_and_recent_first_reorders_inside_folders() {
        let r = games();
        let all = r.all();
        let on_pc = |g: &GameProfile| g.id == "noita" || g.id == "dsp";
        let fav = |g: &GameProfile| g.id == "dsp";
        let last = |g: &GameProfile| match g.id.as_str() {
            "noita" => Some(100),
            "dsp" => Some(50),
            _ => None,
        };
        let c = ctx(&on_pc, &fav, &last);
        assert_eq!(
            headers(&build(&all, "", filter::ON_THIS_PC, Grouping::Franchise, false, false, &c)),
            vec!["Other=dsp,noita"]
        );
        assert_eq!(
            headers(&build(&all, "", filter::ON_THIS_PC, Grouping::Franchise, false, true, &c)),
            vec!["Other=noita,dsp"]
        );
        assert_eq!(
            headers(&build(
                &all,
                "",
                filter::ON_THIS_PC | filter::FAVORITES,
                Grouping::Title,
                false,
                false,
                &c
            )),
            vec!["D=dsp"]
        );
        assert!(build(&all, "", filter::MOD_INSTALLED, Grouping::Title, false, false, &c).is_empty());
    }

    #[test]
    fn typed_words_find_games_by_title_initials_developer_and_a_few_letters() {
        let r = games();
        let find = |q: &str| {
            r.all()
                .into_iter()
                .filter(|g| matches(g, q))
                .map(|g| g.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(find("noit"), vec!["noita"]);
        assert_eq!(find("noi"), vec!["kcd2", "noita"]); // three letters in order are enough, in a long title too
        assert_eq!(find("dyson sph"), vec!["dsp"]);
        assert_eq!(find("kc"), vec!["kcd2"]); // initials
        assert_eq!(find("warhorse"), vec!["kcd2"]); // developer
        assert_eq!(find("nta"), vec!["noita"]); // letters in order
        assert!(find("zzzz").is_empty());
        assert_eq!(find("  ").len(), 5);
        assert_eq!(find("CITIES"), vec!["cs"]);
        assert!(find("noita zzz").is_empty()); // every word has to find something
    }

    #[test]
    fn a_title_that_begins_with_what_was_typed_scores_higher() {
        let r = games();
        let d = r.get("dsp").unwrap();
        assert!(score(d, "dyson") > score(d, "sphere"));
        assert_eq!(score(d, ""), 0);
    }

    #[test]
    fn text_is_normalized_without_accents_or_marks() {
        assert_eq!(normalize("  Héllo,  Wörld! 2 "), "hello world 2");
        assert_eq!(letter_of("  élan"), "É");
        assert_eq!(letter_of("3 Dragons"), "#");
        assert_eq!(letter_of(""), "#");
        assert_eq!(folder_key(Grouping::Developer, "Valve"), "Developer:Valve");
    }
}
