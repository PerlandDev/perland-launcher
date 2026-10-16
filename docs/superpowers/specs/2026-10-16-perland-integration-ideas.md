# Perland integration: idea issues (draft)

- **Status:** brainstormed with the owner on 2026-10-16. These are ideas recorded as issues, not committed work. Each one gets its own spec and plan before anything is built.
- **Why:** the launcher should connect deeply with Perland, but stay simple. Every addition must be easy to understand and useful to a casual player; the launcher must not become deep or complicated.
- **Where the issues go:** launcher work in `PerlandDev/perland-launcher`, server-side work in `PerlandDev/PerlandEngine`, owner-facing work in `PerlandDev/PerlandPanel`. The in-game plugin is filed in PerlandEngine until it gets its own repo.
- **Cross-repo work:** when a launcher issue needs Engine or Panel work, the plan for it says so and the owner is prompted to start a session in that repo.

Placeholders like `[E1]` become real issue links once the issues exist.

## Suggested order

1. **Player accounts:** [E1] then [L1].
2. **Free servers for friends:** [E2], [P1] then [L2].
3. **Public server list:** [E3], [P2] then [L3].
4. **Server news:** [E4], [P3] then [L4].
5. **Perland plugin, first part** (achievements and playtime, Minecraft account link code): [E5], with the launcher side in [L5].
6. **Later:** finding every Perland server plus personal invites ([E6], [P4]), the rest of the plugin, and linking the Minecraft account through Microsoft sign-in ([L5]).

The umbrella issue [L0] in the launcher repo tracks all of it.

---

## [L0] perland-launcher: Perland integration roadmap (umbrella)

**Labels:** enhancement

Tracks the Perland integration ideas brainstormed on 2026-10-16. They are ideas, not commitments: each one gets a spec and a plan before it is built.

**Principle:** go deep with Perland, but keep the launcher simple. Every addition must be easy to understand and must add something for the player.

**Suggested order**
1. Player accounts: [E1], [L1]
2. Free servers for friends: [E2], [P1], [L2]
3. Public server list: [E3], [P2], [L3]
4. Server news: [E4], [P3], [L4]
5. Perland plugin, first part: [E5], [L5]
6. Later: find every Perland server and personal invites ([E6], [P4]), the rest of the plugin, and Minecraft account linking through Microsoft sign-in ([L5])

Related: #32 (achievements, finished by the plugin), #36 (skin editor, waits for Microsoft sign-in).

---

## [L1] perland-launcher: Sign in with Perland and "Your servers"

**Labels:** enhancement · **Needs:** [E1]

**Idea**
A player can link their Perland account in the launcher. It is optional: without it the launcher works as today, including browsing public servers ([L3]).

**Sign-in**
- "Sign in with Perland" opens the browser on Perland's sign-in page. The launcher never sees the password, as with "Sign in with Google" in other apps.
- Players sign in with Discord or an email code ([E1]).

**What a signed-in player gets**
- **Your servers:** one list at the top of the servers screen, holding:
  - servers they own, tagged *Owner*;
  - servers they were invited to, tagged *Invited*. In the first version this means a server they joined through an invite link while signed in. Personal invites to a Perland account come later ([E6]);
  - public servers they follow, with no tag.
- **Following:** pressing Play on a public server follows it automatically, and its menu has *Unfollow*. Following shows the server's news ([L4]). The player never has to learn a "follow" step.

**Later**
- Link the player's Minecraft account to the Perland account once the launcher has Microsoft sign-in ([L5]).

**Non-goals**
- Three separate lists, a social feed, friends lists.

---

## [L2] perland-launcher: Create a server for friends

**Labels:** enhancement · **Needs:** [E2], [L1]

**Idea**
A signed-in player creates a free server in three steps:
1. Pick vanilla or a modpack.
2. Create the server.
3. Share the invite link.

**Behaviour**
- It needs a Perland account with a verified email.
- The free-plan limits ([E2]) are shown up front, in plain words.
- A modpack that needs more memory than a free server has is refused **before** the server is created, with a plain message.
- **Play wakes it:** pressing Play on a sleeping server wakes it and shows "Starting the server…" until it is ready. The player never sees start buttons. Engine already wakes a sleeping server when someone joins.
- The new server appears in *Your servers* tagged *Owner*.

**Open, for the spec**
- Where the modpack comes from: Modrinth (Browse), the player's own instance, or both.

---

## [L3] perland-launcher: Public server list

**Labels:** enhancement · **Needs:** [E3]

**Idea**
Anyone, with or without a Perland account, can find Perland servers whose owner made them public, and join with one click.

**Each server shows**
- name, modpack, players online, and a **Play** button.
- Play works like a join link today: create the instance, install and sync the pack, start the game and connect.

**Order and visibility**
- Default sort: most players online first.
- Sleeping servers are shown as "Sleeping, Play wakes it", after the ones with players online.
- Stopped or broken servers are hidden.

**Sort options:** most players (default), newest, name A–Z.

**Filters**
- With or without mods (read from the pack)
- Minecraft version
- Has room (hides full servers)
- Category (a fixed list, up to 3 per server, set by the owner)
- Language (set by the owner)
- Region / ping

**Layout:** its own screen, separate from Browse, which is for Modrinth content. *Your servers* ([L1]) sits above this list for signed-in players.

**Non-goals:** a mod-loader filter (too technical, and the launcher handles it).

---

## [L4] perland-launcher: Server news on the server card

**Labels:** enhancement · **Needs:** [E4]

**Idea**
A server's news shows in the launcher, on the server's card and its page. There is no separate news tab.

- Followers, owners and invited players see it, and so does anyone looking at a public server.
- A small dot on the card means there is something new.
- A post has a title, short text and an optional picture.
- An event post also has a date and time, and the card shows it ("Saturday 8 pm") until it is over.
- **Automatic post:** when the server's modpack changes, a post such as "Modpack updated: 3 mods added, 1 removed" appears without the owner writing anything.

**Not in the first version:** comments, likes, notifications while the launcher is closed.

---

## [L5] perland-launcher: Link the Minecraft account, and show per-server achievements and playtime

**Labels:** enhancement · **Needs:** [E5], [L1]

**Idea**
- **Minecraft account link:** a signed-in player links their Minecraft account to their Perland account, so servers know who they are in the game (invites, whitelists, friends online).
  - First way: the launcher shows a short code and the player types `/perland link <code>` on a Perland server ([E5]).
  - Later way: automatically, once the launcher has Microsoft sign-in.
- **Achievements and playtime for each server:** with the plugin ([E5]) reporting them, the launcher shows, for example, "12 achievements on Asalith". This finishes the deferred part of #32. Medals can follow once achievements exist.

---

## [E1] PerlandEngine: Player accounts with Discord and email-code sign-in

**Labels:** enhancement · **Pairs with:** perland-launcher [L1]

**Problem**
Perland accounts sign in with GitHub only, and sign-up is closed unless the operator opens it. Players (often 12–25 and casual) mostly have no GitHub account.

**Idea**
- Players can create and sign in to a Perland account with **Discord** or an **email code**. GitHub stays the way in for server operators.
- Perland Launcher can sign a player in through the browser and receives a token for the player's own account (scoped to that player, never the operator's API key).
- Players can open accounts without the operator opening sign-up to everyone. The details are for the spec, and they relate to the sign-up policy (#129) and multi-account abuse (PerlandPanel#128).

**Later:** link a verified Minecraft UUID to the account ([E5], then Microsoft sign-in in the launcher).

---

## [E2] PerlandEngine: Free servers for players

**Labels:** enhancement · **Pairs with:** PerlandPanel [P1], perland-launcher [L2]

**Idea**
A player with a Perland account can create one free server, with or without mods, to play with friends. It is created from the launcher or the panel.

**Starting limits (tunable to real hosting costs)**
- 1 free server per Perland account.
- Up to 10 players at once.
- Memory: about 3 GB for vanilla, about 6 GB for modded. A pack that needs more is refused before creation.
- Disk: about 10 GB.
- Sleeps about 10 minutes after the last player leaves; joining wakes it (already built: idle auto-shutdown with the waker).
- Deleted after 30 days unused. The owner is warned first and can download the world before it goes.

**Abuse protection**
- A verified email before creating a server.
- A cap on how often one account can create and delete servers.
- See PerlandPanel#128 (several accounts per person).

Most of this is a free resource plan on top of the existing plans, quotas and idle policy.

---

## [E3] PerlandEngine: Public server listing

**Labels:** enhancement · **Pairs with:** PerlandPanel [P2], perland-launcher [L3]

**Idea**
An owner can mark a server **public**. Public servers appear in an unauthenticated listing that Perland Launcher reads. It builds on the existing public server page.

**Owner settings**
- Public on/off. Off is the default.
- Categories: up to 3 from a fixed list (Survival, SMP, Creative, Modded/Tech, Magic, Adventure/RPG, Skyblock, Minigames, PvP, Roleplay).
- Language.
- Region, once Perland runs in more than one place.

**Each listed server returns**
- name, icon, modpack, Minecraft version, with or without mods, players online and max players, state (online / sleeping), categories, language, region, and the join details the launcher needs.

**Listing behaviour**
- Sort by players online (default), newest, or name.
- Filter by with/without mods, Minecraft version, has room, category, language and region.
- Sleeping servers are included and marked; stopped or broken servers are left out.

---

## [E4] PerlandEngine: Server news

**Labels:** enhancement · **Pairs with:** PerlandPanel [P3], perland-launcher [L4]

**Idea**
Each server has news posts written by its owner.
- **Post types:** an update, or an event with a date and time.
- **Content:** a title, short text and an optional picture.
- **Automatic post:** created when the server's modpack changes, summarising mods added, removed and updated.
- **Reading:** the launcher reads a server's posts. A public server's posts can be read without signing in.

**Later:** publish the same post to the server's Discord channel through Perlita, so owners write once (see #378).

---

## [E5] PerlandEngine: Perland server plugin / mod

**Labels:** enhancement · **Pairs with:** perland-launcher [L5]

**Idea**
A small Perland add-on that runs inside the Minecraft server and talks to Engine. It lives here until it gets its own repo.

**Platforms:** vanilla servers take a plugin (Paper), while modded servers need a mod per loader. The first version targets what Asalith runs.

**First part**
1. **Achievements and playtime for each server:** the add-on reports each player's advancements and playtime to Perland, and the launcher shows them (perland-launcher #32).
2. **Minecraft account link:** the player types `/perland link <code>` with a code from the launcher, which links their Minecraft account to their Perland account. This only proves ownership on servers that check Minecraft accounts (online mode). Offline-mode servers must not offer it.

**Later**
3. **News in the game:** the latest post is shown on join, and an event start is announced to everyone online ([E4]).
4. **`/invite`:** gives the owner an invite link from inside the game.
5. **Friends online:** "2 friends playing on Asalith" in the launcher. This needs linked Minecraft accounts.

---

## [E6] PerlandEngine: Find every Perland server, and personal invites (later)

**Labels:** enhancement · **Pairs with:** PerlandPanel [P4]

**Idea (later, large)**
Beyond public servers ([E3]):
- players can find servers made on Perland in general;
- owners invite a **Perland account** directly, not only through a link, and the player sees those servers under *Invited* in the launcher.

This is a large product change in Engine and Panel. It is recorded here so it isn't lost, not scheduled.

---

## [P1] PerlandPanel: Free servers for players

**Labels:** enhancement · **Pairs with:** PerlandEngine [E2]

**Idea**
The panel side of free servers:
- Show a player their free-plan limits and how much of them they use.
- Show the warning before an unused server is deleted after 30 days, with a button to download the world.
- Ask for a verified email before creating a free server.

---

## [P2] PerlandPanel: Make a server public

**Labels:** enhancement · **Pairs with:** PerlandEngine [E3]

**Idea**
In a server's settings, the owner can:
- turn **Public** on or off (off by default);
- pick up to 3 categories from the fixed list;
- set the language;
- set the region, once Perland runs in more than one place.

It also shows a preview of how the server appears in the launcher's public list.

---

## [P3] PerlandPanel: Write server news

**Labels:** enhancement · **Pairs with:** PerlandEngine [E4]

**Idea**
A News section for each server, where the owner writes posts:
- **Update:** a title, short text and an optional picture.
- **Event:** the same, plus a date and time.

The list also shows automatic "modpack updated" posts. Owners can edit or delete their posts.

**Later:** a "Also post to Discord" option through Perlita.

---

## [P4] PerlandPanel: Find every Perland server, and personal invites (later)

**Labels:** enhancement · **Pairs with:** PerlandEngine [E6]

**Idea (later, large)**
Panel side of [E6]: invite a Perland account directly to a server and manage those invites, alongside invite links. Recorded so it isn't lost, not scheduled.
