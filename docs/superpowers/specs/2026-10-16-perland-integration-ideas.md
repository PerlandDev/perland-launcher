# Perland integration: idea issues

- **Status:** brainstormed with the owner on 2026-10-16 and posted as issues the same day. These are ideas, not committed work. Each one gets its own spec and plan before anything is built.
- **Why:** the launcher should connect deeply with Perland, but stay simple. Every addition must be easy to understand and useful to a casual player; the launcher must not become deep or complicated.
- **Where the issues go:** launcher work in `PerlandDev/perland-launcher`, server-side work in `PerlandDev/PerlandEngine`, owner-facing work in `PerlandDev/PerlandPanel`. The in-game plugin is filed in PerlandEngine until it gets its own repo.
- **Cross-repo work:** when a launcher issue needs Engine or Panel work, the plan for it says so and the owner is prompted to start a session in that repo.

The GitHub issues are the live record; this file is the snapshot from the brainstorm.

## Suggested order

1. **Player accounts:** [PerlandEngine#429](https://github.com/PerlandDev/PerlandEngine/issues/429) then [perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48).
2. **Free servers for friends:** [PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430), [PerlandPanel#204](https://github.com/PerlandDev/PerlandPanel/issues/204) then [perland-launcher#49](https://github.com/PerlandDev/perland-launcher/issues/49).
3. **Public server list:** [PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431), [PerlandPanel#205](https://github.com/PerlandDev/PerlandPanel/issues/205) then [perland-launcher#50](https://github.com/PerlandDev/perland-launcher/issues/50).
4. **Server news:** [PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432), [PerlandPanel#206](https://github.com/PerlandDev/PerlandPanel/issues/206) then [perland-launcher#51](https://github.com/PerlandDev/perland-launcher/issues/51).
5. **Perland plugin, first part** (achievements and playtime, Minecraft account link code): [PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433), with the launcher side in [perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52).
6. **Later:** finding every Perland server plus personal invites ([PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434), [PerlandPanel#207](https://github.com/PerlandDev/PerlandPanel/issues/207)), the rest of the plugin, and linking the Minecraft account through Microsoft sign-in ([perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52)).

The umbrella issue [perland-launcher#47](https://github.com/PerlandDev/perland-launcher/issues/47) in the launcher repo tracks all of it.

---

## L0: Perland integration roadmap (umbrella) ([perland-launcher#47](https://github.com/PerlandDev/perland-launcher/issues/47))

**Labels:** enhancement

Tracks the Perland integration ideas brainstormed on 2026-10-16. They are ideas, not commitments: each one gets a spec and a plan before it is built.

**Principle:** go deep with Perland, but keep the launcher simple. Every addition must be easy to understand and must add something for the player.

**Suggested order**
1. Player accounts: [PerlandEngine#429](https://github.com/PerlandDev/PerlandEngine/issues/429), [perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48)
2. Free servers for friends: [PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430), [PerlandPanel#204](https://github.com/PerlandDev/PerlandPanel/issues/204), [perland-launcher#49](https://github.com/PerlandDev/perland-launcher/issues/49)
3. Public server list: [PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431), [PerlandPanel#205](https://github.com/PerlandDev/PerlandPanel/issues/205), [perland-launcher#50](https://github.com/PerlandDev/perland-launcher/issues/50)
4. Server news: [PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432), [PerlandPanel#206](https://github.com/PerlandDev/PerlandPanel/issues/206), [perland-launcher#51](https://github.com/PerlandDev/perland-launcher/issues/51)
5. Perland plugin, first part: [PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433), [perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52)
6. Later: find every Perland server and personal invites ([PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434), [PerlandPanel#207](https://github.com/PerlandDev/PerlandPanel/issues/207)), the rest of the plugin, and Minecraft account linking through Microsoft sign-in ([perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52))

Related: #32 (achievements, finished by the plugin), #36 (skin editor, waits for Microsoft sign-in).

---

## L1: Sign in with Perland and "Your servers" ([perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48))

**Labels:** enhancement · **Needs:** [PerlandEngine#429](https://github.com/PerlandDev/PerlandEngine/issues/429)

**Idea**
A player can link their Perland account in the launcher. It is optional: without it the launcher works as today, including browsing public servers ([perland-launcher#50](https://github.com/PerlandDev/perland-launcher/issues/50)).

**Sign-in**
- "Sign in with Perland" opens the browser on Perland's sign-in page. The launcher never sees the password, as with "Sign in with Google" in other apps.
- Players sign in with Discord or an email code ([PerlandEngine#429](https://github.com/PerlandDev/PerlandEngine/issues/429)).

**What a signed-in player gets**
- **Your servers:** one list at the top of the servers screen, holding:
  - servers they own, tagged *Owner*;
  - servers they were invited to, tagged *Invited*. In the first version this means a server they joined through an invite link while signed in. Personal invites to a Perland account come later ([PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434));
  - public servers they follow, with no tag.
- **Following:** pressing Play on a public server follows it automatically, and its menu has *Unfollow*. Following shows the server's news ([perland-launcher#51](https://github.com/PerlandDev/perland-launcher/issues/51)). The player never has to learn a "follow" step.

**Later**
- Link the player's Minecraft account to the Perland account once the launcher has Microsoft sign-in ([perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52)).

**Non-goals**
- Three separate lists, a social feed, friends lists.

---

## L2: Create a server for friends ([perland-launcher#49](https://github.com/PerlandDev/perland-launcher/issues/49))

**Labels:** enhancement · **Needs:** [PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430), [perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48)

**Idea**
A signed-in player creates a free server in three steps:
1. Pick vanilla or a modpack.
2. Create the server.
3. Share the invite link.

**Behaviour**
- It needs a Perland account with a verified email.
- The free-plan limits ([PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430)) are shown up front, in plain words.
- A modpack that needs more memory than a free server has is refused **before** the server is created, with a plain message.
- **Play wakes it:** pressing Play on a sleeping server wakes it and shows "Starting the server…" until it is ready. The player never sees start buttons. Engine already wakes a sleeping server when someone joins.
- The new server appears in *Your servers* tagged *Owner*.

**Open, for the spec**
- Where the modpack comes from: Modrinth (Browse), the player's own instance, or both.

---

## L3: Public server list ([perland-launcher#50](https://github.com/PerlandDev/perland-launcher/issues/50))

**Labels:** enhancement · **Needs:** [PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431)

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

**Layout:** its own screen, separate from Browse, which is for Modrinth content. *Your servers* ([perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48)) sits above this list for signed-in players.

**Non-goals:** a mod-loader filter (too technical, and the launcher handles it).

---

## L4: Server news on the server card ([perland-launcher#51](https://github.com/PerlandDev/perland-launcher/issues/51))

**Labels:** enhancement · **Needs:** [PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432)

**Idea**
A server's news shows in the launcher, on the server's card and its page. There is no separate news tab.

- Followers, owners and invited players see it, and so does anyone looking at a public server.
- A small dot on the card means there is something new.
- A post has a title, short text and an optional picture.
- An event post also has a date and time, and the card shows it ("Saturday 8 pm") until it is over.
- **Automatic post:** when the server's modpack changes, a post such as "Modpack updated: 3 mods added, 1 removed" appears without the owner writing anything.

**Not in the first version:** comments, likes, notifications while the launcher is closed.

---

## L5: Link the Minecraft account, and show per-server achievements and playtime ([perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52))

**Labels:** enhancement · **Needs:** [PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433), [perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48)

**Idea**
- **Minecraft account link:** a signed-in player links their Minecraft account to their Perland account, so servers know who they are in the game (invites, whitelists, friends online).
  - First way: the launcher shows a short code and the player types `/perland link <code>` on a Perland server ([PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433)).
  - Later way: automatically, once the launcher has Microsoft sign-in.
- **Achievements and playtime for each server:** with the plugin ([PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433)) reporting them, the launcher shows, for example, "12 achievements on Asalith". This finishes the deferred part of #32. Medals can follow once achievements exist.

---

## E1: Player accounts with Discord and email-code sign-in ([PerlandEngine#429](https://github.com/PerlandDev/PerlandEngine/issues/429))

**Labels:** enhancement · **Pairs with:** [perland-launcher#48](https://github.com/PerlandDev/perland-launcher/issues/48)

**Problem**
Perland accounts sign in with GitHub only, and sign-up is closed unless the operator opens it. Players (often 12–25 and casual) mostly have no GitHub account.

**Idea**
- Players can create and sign in to a Perland account with **Discord** or an **email code**. GitHub stays the way in for server operators.
- Perland Launcher can sign a player in through the browser and receives a token for the player's own account (scoped to that player, never the operator's API key).
- Players can open accounts without the operator opening sign-up to everyone. The details are for the spec, and they relate to the sign-up policy (#129) and multi-account abuse (PerlandPanel#128).

**Later:** link a verified Minecraft UUID to the account ([PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433), then Microsoft sign-in in the launcher).

---

## E2: Free servers for players ([PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430))

**Labels:** enhancement · **Pairs with:** [PerlandPanel#204](https://github.com/PerlandDev/PerlandPanel/issues/204), [perland-launcher#49](https://github.com/PerlandDev/perland-launcher/issues/49)

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

## E3: Public server listing ([PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431))

**Labels:** enhancement · **Pairs with:** [PerlandPanel#205](https://github.com/PerlandDev/PerlandPanel/issues/205), [perland-launcher#50](https://github.com/PerlandDev/perland-launcher/issues/50)

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

## E4: Server news ([PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432))

**Labels:** enhancement · **Pairs with:** [PerlandPanel#206](https://github.com/PerlandDev/PerlandPanel/issues/206), [perland-launcher#51](https://github.com/PerlandDev/perland-launcher/issues/51)

**Idea**
Each server has news posts written by its owner.
- **Post types:** an update, or an event with a date and time.
- **Content:** a title, short text and an optional picture.
- **Automatic post:** created when the server's modpack changes, summarising mods added, removed and updated.
- **Reading:** the launcher reads a server's posts. A public server's posts can be read without signing in.

**Later:** publish the same post to the server's Discord channel through Perlita, so owners write once (see #378).

---

## E5: Perland server plugin / mod ([PerlandEngine#433](https://github.com/PerlandDev/PerlandEngine/issues/433))

**Labels:** enhancement · **Pairs with:** [perland-launcher#52](https://github.com/PerlandDev/perland-launcher/issues/52)

**Idea**
A small Perland add-on that runs inside the Minecraft server and talks to Engine. It lives here until it gets its own repo.

**Platforms:** vanilla servers take a plugin (Paper), while modded servers need a mod per loader. The first version targets what Asalith runs.

**First part**
1. **Achievements and playtime for each server:** the add-on reports each player's advancements and playtime to Perland, and the launcher shows them (perland-launcher #32).
2. **Minecraft account link:** the player types `/perland link <code>` with a code from the launcher, which links their Minecraft account to their Perland account. This only proves ownership on servers that check Minecraft accounts (online mode). Offline-mode servers must not offer it.

**Later**
3. **News in the game:** the latest post is shown on join, and an event start is announced to everyone online ([PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432)).
4. **`/invite`:** gives the owner an invite link from inside the game.
5. **Friends online:** "2 friends playing on Asalith" in the launcher. This needs linked Minecraft accounts.

---

## E6: Find every Perland server, and personal invites (later) ([PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434))

**Labels:** enhancement · **Pairs with:** [PerlandPanel#207](https://github.com/PerlandDev/PerlandPanel/issues/207)

**Idea (later, large)**
Beyond public servers ([PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431)):
- players can find servers made on Perland in general;
- owners invite a **Perland account** directly, not only through a link, and the player sees those servers under *Invited* in the launcher.

This is a large product change in Engine and Panel. It is recorded here so it isn't lost, not scheduled.

---

## P1: Free servers for players ([PerlandPanel#204](https://github.com/PerlandDev/PerlandPanel/issues/204))

**Labels:** enhancement · **Pairs with:** [PerlandEngine#430](https://github.com/PerlandDev/PerlandEngine/issues/430)

**Idea**
The panel side of free servers:
- Show a player their free-plan limits and how much of them they use.
- Show the warning before an unused server is deleted after 30 days, with a button to download the world.
- Ask for a verified email before creating a free server.

---

## P2: Make a server public ([PerlandPanel#205](https://github.com/PerlandDev/PerlandPanel/issues/205))

**Labels:** enhancement · **Pairs with:** [PerlandEngine#431](https://github.com/PerlandDev/PerlandEngine/issues/431)

**Idea**
In a server's settings, the owner can:
- turn **Public** on or off (off by default);
- pick up to 3 categories from the fixed list;
- set the language;
- set the region, once Perland runs in more than one place.

It also shows a preview of how the server appears in the launcher's public list.

---

## P3: Write server news ([PerlandPanel#206](https://github.com/PerlandDev/PerlandPanel/issues/206))

**Labels:** enhancement · **Pairs with:** [PerlandEngine#432](https://github.com/PerlandDev/PerlandEngine/issues/432)

**Idea**
A News section for each server, where the owner writes posts:
- **Update:** a title, short text and an optional picture.
- **Event:** the same, plus a date and time.

The list also shows automatic "modpack updated" posts. Owners can edit or delete their posts.

**Later:** a "Also post to Discord" option through Perlita.

---

## P4: Find every Perland server, and personal invites (later) ([PerlandPanel#207](https://github.com/PerlandDev/PerlandPanel/issues/207))

**Labels:** enhancement · **Pairs with:** [PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434)

**Idea (later, large)**
Panel side of [PerlandEngine#434](https://github.com/PerlandDev/PerlandEngine/issues/434): invite a Perland account directly to a server and manage those invites, alongside invite links. Recorded so it isn't lost, not scheduled.
