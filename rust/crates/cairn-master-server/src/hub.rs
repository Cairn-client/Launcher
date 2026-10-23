// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! Who is online with Cairn, and who may see and invite whom (the rules are in `cairn_protocol::presence_api`). In memory only: nothing about a
//! player outlives their socket. A player is visible to another only when each lists the other as a Steam friend.

use cairn_protocol::presence_api::{
    self as api, PresenceFriend, PresenceHello, PresenceInvite, PresenceInvited, PresenceMessage, PresenceStatus,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc::UnboundedSender;

/// What a peer's socket is told to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Message(Box<PresenceMessage>),
    /// Close the socket (the same player signed in elsewhere).
    Close,
}

pub struct Peer {
    pub id: String,
    pub name: String,
    friends: HashSet<String>,
    status: Mutex<PresenceStatus>,
    tx: UnboundedSender<Outgoing>,
    invites: Mutex<VecDeque<u64>>,
}

impl Peer {
    /// Tells this peer why something it asked for was refused.
    pub fn send_refused(&self, reason: &str) -> bool {
        let mut m = message(api::message_type::REFUSED);
        m.reason = Some(reason.into());
        self.tx.send(Outgoing::Message(Box::new(m))).is_ok()
    }

    fn send(&self, m: PresenceMessage) {
        // a socket that cannot be written to is closing; its own loop ends it
        let _ = self.tx.send(Outgoing::Message(Box::new(m)));
    }

    fn status(&self) -> PresenceStatus {
        lock(&self.status).clone()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
pub struct Hub {
    peers: Mutex<HashMap<String, Arc<Peer>>>,
}

fn mutual(a: &Peer, b: &Peer) -> bool {
    a.friends.contains(&b.id) && b.friends.contains(&a.id)
}

fn message(kind: &str) -> PresenceMessage {
    PresenceMessage {
        kind: kind.into(),
        api_version: api::VERSION,
        ..Default::default()
    }
}

fn friend_message(p: &Peer, online: bool) -> PresenceMessage {
    let s = p.status();
    let mut m = message(api::message_type::FRIEND);
    m.friend = Some(PresenceFriend {
        id: p.id.clone(),
        name: p.name.clone(),
        online,
        activity: s.activity,
        game: s.game,
    });
    m
}

fn clean_status(s: &PresenceStatus) -> PresenceStatus {
    PresenceStatus {
        activity: if api::activity::valid(Some(&s.activity)) {
            s.activity.clone()
        } else {
            api::activity::ONLINE.into()
        },
        game: api::clean_text(Some(&s.game), api::MAX_GAME_LENGTH),
    }
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn online(&self) -> usize {
        lock(&self.peers).len()
    }

    /// A hello: the peer when accepted (its friends already told), else the reason it was refused.
    pub fn hello(&self, h: &PresenceHello, tx: UnboundedSender<Outgoing>) -> Result<Arc<Peer>, String> {
        if !api::valid_id(Some(&h.id)) {
            return Err("That is not a Steam id.".into());
        }
        if h.friends.len() > api::MAX_FRIENDS {
            return Err("Too many friends listed.".into());
        }
        let name = api::clean_text(Some(&h.name), api::MAX_NAME_LENGTH);
        if name.is_empty() {
            return Err("A name is needed.".into());
        }
        let friends: HashSet<String> = h
            .friends
            .iter()
            .filter(|f| api::valid_id(Some(f)) && **f != h.id)
            .cloned()
            .collect();
        let peer = Arc::new(Peer {
            id: h.id.clone(),
            name,
            friends,
            status: Mutex::new(clean_status(&h.status)),
            tx,
            invites: Mutex::new(VecDeque::new()),
        });
        let (replaced, mutual_peers) = {
            let mut peers = lock(&self.peers);
            let replaced = peers.insert(h.id.clone(), peer.clone());
            let others: Vec<Arc<Peer>> = peers
                .values()
                .filter(|p| !Arc::ptr_eq(p, &peer) && mutual(&peer, p))
                .cloned()
                .collect();
            (replaced, others)
        };
        if let Some(old) = replaced {
            let _ = old.tx.send(Outgoing::Close); // the same player signed in elsewhere: the older socket goes
        }
        peer.send(message(api::message_type::WELCOME));
        for p in mutual_peers {
            peer.send(friend_message(&p, true));
            p.send(friend_message(&peer, true));
        }
        Ok(peer)
    }

    pub fn status(&self, peer: &Arc<Peer>, status: &PresenceStatus) {
        let others = {
            let peers = lock(&self.peers);
            if !peers.get(&peer.id).is_some_and(|cur| Arc::ptr_eq(cur, peer)) {
                return;
            }
            *lock(&peer.status) = clean_status(status);
            peers
                .values()
                .filter(|p| !Arc::ptr_eq(p, peer) && mutual(peer, p))
                .cloned()
                .collect::<Vec<_>>()
        };
        for p in others {
            p.send(friend_message(peer, true));
        }
    }

    /// An invitation: `None` when delivered, else the reason (sent back to the inviter). `now` is in milliseconds.
    pub fn invite(&self, from: &Arc<Peer>, invite: &PresenceInvite, now: u64) -> Option<String> {
        if !api::valid_id(Some(&invite.to)) {
            return Some("That is not a Steam id.".into());
        }
        let game = api::clean_text(Some(&invite.game), api::MAX_GAME_LENGTH);
        let code = api::clean_text(Some(&invite.code), api::MAX_CODE_LENGTH);
        if code.is_empty() {
            return Some("An invitation needs a code.".into());
        }
        let to = {
            let peers = lock(&self.peers);
            if !peers.get(&from.id).is_some_and(|cur| Arc::ptr_eq(cur, from)) {
                return Some("You are not connected.".into());
            }
            let mut sent = lock(&from.invites);
            while sent.front().is_some_and(|t| now.saturating_sub(*t) > 60_000) {
                sent.pop_front();
            }
            if sent.len() >= api::INVITES_PER_MINUTE as usize {
                return Some("Too many invitations; wait a minute.".into());
            }
            let Some(to) = peers.get(&invite.to).filter(|to| mutual(from, to)).cloned() else {
                return Some("That friend is not online with Cairn.".into());
            };
            sent.push_back(now);
            to
        };
        let mut m = message(api::message_type::INVITED);
        m.invited = Some(PresenceInvited {
            from: from.id.clone(),
            name: from.name.clone(),
            game,
            code,
        });
        to.send(m);
        None
    }

    pub fn leave(&self, peer: &Arc<Peer>) {
        let others = {
            let mut peers = lock(&self.peers);
            if !peers.get(&peer.id).is_some_and(|cur| Arc::ptr_eq(cur, peer)) {
                return; // already replaced by a newer socket
            }
            peers.remove(&peer.id);
            peers.values().filter(|p| mutual(peer, p)).cloned().collect::<Vec<_>>()
        };
        for p in others {
            p.send(friend_message(peer, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    const A: &str = "76561198000000001";
    const B: &str = "76561198000000002";
    const C: &str = "76561198000000003";

    struct Client {
        rx: UnboundedReceiver<Outgoing>,
        peer: Arc<Peer>,
    }

    impl Client {
        fn got(&mut self) -> Vec<Outgoing> {
            let mut v = vec![];
            while let Ok(m) = self.rx.try_recv() {
                v.push(m);
            }
            v
        }
        fn messages(&mut self) -> Vec<PresenceMessage> {
            self.got()
                .into_iter()
                .filter_map(|o| if let Outgoing::Message(m) = o { Some(*m) } else { None })
                .collect()
        }
    }

    fn join(hub: &Hub, id: &str, name: &str, friends: &[&str], activity: &str, game: &str) -> Client {
        let (tx, rx) = unbounded_channel();
        let hello = PresenceHello {
            id: id.into(),
            name: name.into(),
            friends: friends.iter().map(|f| f.to_string()).collect(),
            status: PresenceStatus {
                activity: activity.into(),
                game: game.into(),
            },
        };
        Client {
            rx,
            peer: hub.hello(&hello, tx).unwrap(),
        }
    }

    fn friends_of(msgs: &[PresenceMessage]) -> Vec<PresenceFriend> {
        msgs.iter()
            .filter(|m| m.kind == api::message_type::FRIEND)
            .filter_map(|m| m.friend.clone())
            .collect()
    }

    #[test]
    fn mutual_friends_see_each_other_come_online() {
        let hub = Hub::new();
        let mut a = join(&hub, A, "Alice", &[B], "online", "");
        let mut b = join(&hub, B, "Bob", &[A], "playing", "Kingdom Come: Deliverance II");
        let am = a.messages();
        assert!(am.iter().any(|m| m.kind == api::message_type::WELCOME));
        assert!(friends_of(&am)
            .iter()
            .any(|f| f.id == B && f.online && f.activity == "playing" && f.game == "Kingdom Come: Deliverance II"));
        assert!(friends_of(&b.messages()).iter().any(|f| f.id == A && f.online));
    }

    #[test]
    fn a_one_sided_friendship_shows_nothing() {
        let hub = Hub::new();
        let mut a = join(&hub, A, "Alice", &[B], "online", "");
        let mut b = join(&hub, B, "Bob", &[C], "online", ""); // Bob does not list Alice
        assert!(friends_of(&a.messages()).is_empty() && friends_of(&b.messages()).is_empty());
    }

    #[test]
    fn a_status_change_reaches_friends_and_a_leave_says_offline() {
        let hub = Hub::new();
        let mut a = join(&hub, A, "Alice", &[B], "online", "");
        let b = join(&hub, B, "Bob", &[A], "online", "");
        a.got();
        hub.status(
            &b.peer,
            &PresenceStatus {
                activity: "hosting".into(),
                game: "KCD2".into(),
            },
        );
        assert!(friends_of(&a.messages())
            .iter()
            .any(|f| f.id == B && f.activity == "hosting" && f.game == "KCD2"));
        hub.leave(&b.peer);
        assert!(friends_of(&a.messages()).iter().any(|f| f.id == B && !f.online));
    }

    #[test]
    fn an_invitation_reaches_only_an_online_mutual_friend() {
        let hub = Hub::new();
        let a = join(&hub, A, "Alice", &[B, C], "online", "");
        let mut b = join(&hub, B, "Bob", &[A], "online", "");
        let mut c = join(&hub, C, "Carol", &[], "online", ""); // does not list Alice back
        b.got();
        c.got();
        let inv = |to: &str, code: &str| PresenceInvite {
            to: to.into(),
            game: "KCD2".into(),
            code: code.into(),
        };
        assert_eq!(hub.invite(&a.peer, &inv(B, "ABC123"), 0), None);
        let invited = b
            .messages()
            .into_iter()
            .find(|m| m.kind == api::message_type::INVITED)
            .unwrap()
            .invited
            .unwrap();
        assert_eq!(
            (
                invited.from.as_str(),
                invited.name.as_str(),
                invited.game.as_str(),
                invited.code.as_str()
            ),
            (A, "Alice", "KCD2", "ABC123")
        );
        assert!(hub.invite(&a.peer, &inv(C, "x"), 0).is_some());
        assert!(c.messages().iter().all(|m| m.kind != api::message_type::INVITED));
        assert!(hub.invite(&a.peer, &inv(B, ""), 0).is_some());
    }

    #[test]
    fn invitations_are_rate_limited() {
        let hub = Hub::new();
        let a = join(&hub, A, "Alice", &[B], "online", "");
        let _b = join(&hub, B, "Bob", &[A], "online", "");
        let inv = PresenceInvite {
            to: B.into(),
            game: String::new(),
            code: "c".into(),
        };
        for _ in 0..api::INVITES_PER_MINUTE {
            assert_eq!(hub.invite(&a.peer, &inv, 1000), None);
        }
        assert!(hub.invite(&a.peer, &inv, 1000).is_some());
        assert_eq!(hub.invite(&a.peer, &inv, 1000 + 120_000), None);
    }

    #[test]
    fn signing_in_again_replaces_the_older_socket() {
        let hub = Hub::new();
        let mut first = join(&hub, A, "Alice", &[B], "online", "");
        let mut second = join(&hub, A, "Alice", &[B], "online", "");
        assert!(first.got().contains(&Outgoing::Close));
        hub.leave(&first.peer); // the old socket's own cleanup must not take the new one offline
        assert_eq!(hub.online(), 1);
        assert!(!second.got().contains(&Outgoing::Close));
    }

    #[test]
    fn a_bad_hello_is_refused() {
        let hub = Hub::new();
        for (id, name) in [("1234", "Alice"), (A, "  ")] {
            let (tx, _rx) = unbounded_channel();
            let h = PresenceHello {
                id: id.into(),
                name: name.into(),
                ..Default::default()
            };
            assert!(hub.hello(&h, tx).is_err());
        }
    }

    #[test]
    fn ids_and_text_are_cleaned_for_the_wire() {
        assert!(api::valid_id(Some(A)));
        assert!(!api::valid_id(Some("76561198000000")));
        assert!(!api::valid_id(Some("1234567890123456a")));
        assert_eq!(api::clean_text(Some("  Bo\u{7}b "), 48), "Bob");
        assert_eq!(api::clean_text(Some("abcdefgh"), 5).chars().count(), 5);
    }
}
