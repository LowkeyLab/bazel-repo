# Weapon Windfury Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let active weapon Windfury contribute to Hero attack allowance while preserving attack history and existing combat timing.

**Architecture:** Read current weapon Windfury through the existing authoritative equipment lookup. Combine it with character-owned Windfury in the shared exhaustion check, which already serves validation, snapshots, and Frozen thawing. Add no serialized state or generic keyword inheritance.

**Tech Stack:** Rust, Bevy ECS, Bazel/Aspect, Nix development environment.

---

## Approved contract and workspace

Read `docs/superpowers/specs/2026-09-16-hearthstone-weapon-windfury-design.md` before execution.
The current dedicated worktree is based on `8ac9c232`, with the approved spec committed as
`1a30a42`. Keep checkpoint schema 17; compatibility is structural, not an assurance that old
binaries with inactive weapon Windfury produce the same gameplay.

Use the repository Nix environment because `aspect` and `bazel` are absent from the default
PATH. After EVERY source-edit batch run the following immediately, before any formatting:

```bash
nix develop --command bazel run //:gazelle
```

Never manually change BUILD files before Gazelle. The proposed changes use existing source
files and test includes, so no BUILD changes are expected.

## File responsibilities

| File                                                                      | Change                                                                |
| ------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `hearthstone_simulator/simulator/weapon.rs`                               | Narrow live weapon-Windfury query                                     |
| `hearthstone_simulator/simulator/simulation_action_validation.rs`         | Combine own and weapon Windfury for allowance                         |
| `hearthstone_simulator/simulator/simulation_tests_actions.rs`             | Regression fixtures beside existing weapon tests; reuse local helpers |
| `hearthstone_simulator/README.md`                                         | Document supported behavior and schema semantics                      |
| `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`                        | Record completed slice and actual verification                        |
| `hearthstone_simulator/RULEBOOK_CONFORMANCE.md`                           | Separate supported behavior from explicit timing policies             |
| `docs/superpowers/specs/2026-09-16-hearthstone-weapon-windfury-design.md` | Update implementation status after verification                       |

Do not modify the generic `aura::has_keyword`, snapshot structures, resolver operations,
durability payment, or checkpoint version.

## Task 1: Live allowance and first regression

**Modify:** `hearthstone_simulator/simulator/simulation_tests_actions.rs`,
`hearthstone_simulator/simulator/weapon.rs`,
`hearthstone_simulator/simulator/simulation_action_validation.rs`.

- [x] Add the following regression beside the existing weapon tests. All helpers used here
      already exist in that file or its imported test support.

```rust
#[test]
fn weapon_windfury_allows_two_attacks_without_resetting_history() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Wind blade", 0, 3, 3).with_keyword(Keyword::Windfury),
    ]);
    let weapon = weapon_play(&mut sim);
    let attacker = hero(&mut sim, PlayerId::One);
    let defender = hero(&mut sim, PlayerId::Two);
    let action = GameAction::Attack { player: PlayerId::One, attacker, defender };
    for spent in 0..2 {
        assert_windfury_legality(&mut sim, attacker, &action, true);
        assert_eq!(windfury_attack_state(&sim, attacker).attacks_this_turn, spent);
        sim.apply(action.clone()).unwrap();
    }
    assert_windfury_legality(&mut sim, attacker, &action, false);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.players[1].health, 24);
    assert_eq!(snapshot.objects.iter().find(|o| o.id == weapon).unwrap().durability, Some(1));
}
```

- [x] Run Gazelle, then the focused test. Expected failure before implementation: after the
      first attack, the legal-action/exhaustion assertion fails because allowance is still one.

```bash
nix develop --command bazel run //:gazelle
nix develop --command aspect test //hearthstone_simulator/simulator:simulator_test --test_filter=weapon_windfury
```

- [x] Add this narrow query to `weapon.rs`; fully qualified names avoid unnecessary import edits.

```rust
pub(crate) fn grants_windfury(world: &World, entity: Entity) -> bool {
    attack_weapon(world, entity)
        .and_then(|(_, id)| game_entity(world, id))
        .is_some_and(|weapon| {
            crate::aura::has_keyword(world, weapon, crate::Keyword::Windfury)
        })
}
```

- [x] Replace only the allowance initializer in `attack_exhausted` with:

```rust
let allowance = if has_keyword(world, entity, Keyword::Windfury)
    || crate::weapon::grants_windfury(world, entity)
{
    2
} else {
    1
};
```

- [x] Run Gazelle immediately, then the same focused test; expect PASS. Keep existing
      readiness and Minion Charge/Rush logic untouched.

## Task 2: Equipment, keyword, and turn transitions

**Modify:** `hearthstone_simulator/simulator/simulation_tests_actions.rs`.

- [x] Add this replacement matrix. It proves that only current sources determine allowance,
      and that switching weapons neither clears attack history nor stacks Windfury.

```rust
#[test]
fn weapon_windfury_replacement_preserves_spent_attacks() {
    for old_windfury in [false, true] {
        for new_windfury in [false, true] {
            for own_windfury in [false, true] {
                let mut old = Card::weapon("Old", 0, 1, 4);
                let mut new = Card::weapon("New", 0, 1, 4);
                if old_windfury { old = old.with_keyword(Keyword::Windfury); }
                if new_windfury { new = new.with_keyword(Keyword::Windfury); }
                let mut sim = weapon_fixture(vec![old, new]);
                weapon_play(&mut sim);
                let attacker = hero(&mut sim, PlayerId::One);
                let defender = hero(&mut sim, PlayerId::Two);
                let action = GameAction::Attack { player: PlayerId::One, attacker, defender };
                if own_windfury {
                    keyword_grant(&mut sim, attacker, Keyword::Windfury, EnchantmentDuration::Permanent);
                }
                sim.apply(action.clone()).unwrap();
                weapon_play(&mut sim);
                assert_eq!(windfury_attack_state(&sim, attacker).attacks_this_turn, 1);
                let allowed = new_windfury || own_windfury;
                assert_windfury_legality(&mut sim, attacker, &action, allowed);
                if allowed {
                    sim.apply(action.clone()).unwrap();
                    keyword_grant(&mut sim, attacker, Keyword::Windfury, EnchantmentDuration::Permanent);
                    assert_windfury_legality(&mut sim, attacker, &action, false);
                }
            }
        }
    }
}
```

- [x] Add this dynamic-grant and thaw regression. Existing `keyword_grant`, `freeze`,
      `is_frozen`, and `end_active_turn` helpers must be reused.

```rust
#[test]
fn weapon_windfury_live_grants_control_thaw_before_expiration() {
    for granted in [false, true] {
        let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 2, 4)]);
        let weapon = weapon_play(&mut sim);
        let attacker = hero(&mut sim, PlayerId::One);
        weapon_attack(&mut sim);
        if granted {
            keyword_grant(&mut sim, weapon, Keyword::Windfury,
                EnchantmentDuration::EndOfTurn(PlayerId::One));
        }
        freeze(&mut sim, attacker);
        end_active_turn(&mut sim);
        assert_eq!(is_frozen(&sim, attacker), !granted);
        let entity = game_entity(sim.app.world(), weapon).unwrap();
        assert!(!crate::aura::has_keyword(sim.app.world(), entity, Keyword::Windfury));
    }
}
```

- [x] Extend coverage using these concrete setup operations. Keep separate named tests for
      each row of the acceptance matrix below so failures identify the transition involved.

```rust
// Give personal Attack without a weapon; update base and current stats so aura refresh
// preserves the fixture. Reuse this for breakage and zero-Attack weapon tests.
let entity = game_entity(sim.app.world(), attacker).unwrap();
sim.app.world_mut().get_mut::<crate::BaseStats>(entity).unwrap().attack = 2;
sim.app.world_mut().get_mut::<CurrentStats>(entity).unwrap().attack = 2;

// Remove only the weapon's current Windfury through the production enchantment path.
super::effect_executor::attach_keyword_modifier(
    sim.app.world_mut(), PlayerId::One, weapon,
    crate::KeywordModifier {
        keyword: Keyword::Windfury, granted: false, silence_removable: true,
    }, EnchantmentDuration::Permanent,
).unwrap();

// Gain or regain current weapon Windfury without changing attack history.
keyword_grant(&mut sim, weapon, Keyword::Windfury, EnchantmentDuration::Permanent);
```

| Named test suffix after `weapon_windfury_`  | Setup and exact expectations                                                                                                                                                                                   |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unarmed_history_and_zero_attack_weapon`    | Give Hero 2 Attack; attack unarmed; equip 0-Attack Windfury weapon; legality true with one spent; second attack succeeds; third rejected.                                                                      |
| `breakage_removes_only_weapon_contribution` | Hero has 2 personal Attack; equip 1-durability Windfury weapon; attack once; weapon slot becomes None; second declaration rejected unless own Hero Windfury is granted.                                        |
| `removal_and_regain_preserve_history`       | Attack once with 4-durability Windfury weapon; remove its keyword; legality false; regain it; legality true; attack again; remove/regain once more; legality stays false.                                      |
| `off_turn_and_refresh`                      | Attack twice; end turn; weapon query false for that Hero; end opponent turn; attacks spent zero and legality true.                                                                                             |
| `unrelated_keywords_stay_local`             | After one attack, grant each of Taunt, Stealth, Immune, Charge, Rush, Frozen, DivineShield, Lifesteal, Poisonous to an ordinary weapon; weapon Windfury query stays false and second attack stays unavailable. |

For each row, construct the same `action` as Task 1 and call `assert_windfury_legality` at idle
boundaries. Its existing checks cover enumeration purity, validation, exhaustion, and atomic
rejection; avoid using it during suspended resolution or while Frozen blocks a declaration.

- [x] Run Gazelle immediately after each source batch. Run the focused `weapon_windfury`
      filter after these tests, then the existing `windfury` filter to include Minion regressions.

```bash
nix develop --command aspect test //hearthstone_simulator/simulator:simulator_test --test_filter=windfury
```

Expected: all cases pass with the Task 1 production change. Investigate any failure against
shared timing semantics before adding production changes.

## Task 3: Suspended replacement and accepted attacks

**Modify:** `hearthstone_simulator/simulator/simulation_tests_actions.rs`.

- [x] Add a suspension matrix using existing `weapon_pause` and `weapon_restore_and_finish`.
      It must observe the shared exhaustion query directly during suspension because actions
      are unavailable until the pending choice resolves.

```rust
#[test]
fn weapon_windfury_suspended_replacement_uses_only_active_weapon() {
    for old_windfury in [false, true] {
        for new_windfury in [false, true] {
            let mut old = Card::weapon("Old", 0, 1, 4);
            let mut new = Card::weapon("New", 0, 1, 4).with_effects(vec![weapon_pause()]);
            if old_windfury { old = old.with_keyword(Keyword::Windfury); }
            if new_windfury { new = new.with_keyword(Keyword::Windfury); }
            let mut sim = weapon_fixture(vec![old, new]);
            let old_id = weapon_play(&mut sim);
            let attacker = hero(&mut sim, PlayerId::One);
            weapon_attack(&mut sim);
            let new_id = weapon_play(&mut sim);
            assert!(sim.pending_choice().is_some());
            let snapshot = sim.snapshot();
            assert_eq!(snapshot.players[0].weapon, Some(new_id));
            assert_eq!(snapshot.objects.iter().find(|o| o.id == old_id).unwrap().zone, Zone::Play);
            assert_eq!(windfury_snapshot_exhausted(&mut sim, attacker), !new_windfury);
            weapon_restore_and_finish(&mut sim);
            assert_eq!(windfury_snapshot_exhausted(&mut sim, attacker), !new_windfury);
        }
    }
}
```

- [x] Add the accepted-second-attack regression below. Compare explicit traces as well as
      checkpoints if the existing restore helper's checkpoint equality ever excludes trace data.

```rust
#[test]
fn weapon_windfury_loss_during_second_attack_preserves_continuation() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Blade", 0, 2, 4).with_keyword(Keyword::Windfury),
    ]);
    let weapon = weapon_play(&mut sim);
    let attacker = hero(&mut sim, PlayerId::One);
    weapon_attack(&mut sim);
    let mut trigger = self_event_trigger(EventKind::Attack, vec![weapon_pause()]);
    trigger.conditions.clear();
    let entity = game_entity(sim.app.world(), attacker).unwrap();
    sim.app.world_mut().entity_mut(entity).insert(RuntimeTriggers(vec![trigger]));
    weapon_attack(&mut sim);
    assert!(sim.pending_choice().is_some());
    super::effect_executor::attach_keyword_modifier(
        sim.app.world_mut(), PlayerId::One, weapon,
        crate::KeywordModifier {
            keyword: Keyword::Windfury, granted: false, silence_removable: true,
        }, EnchantmentDuration::Permanent,
    ).unwrap();
    weapon_restore_and_finish(&mut sim);
    assert_eq!(windfury_attack_state(&sim, attacker).attacks_this_turn, 2);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.players[1].health, 26);
    assert_eq!(snapshot.objects.iter().find(|o| o.id == weapon).unwrap().durability, Some(2));
    assert!(windfury_snapshot_exhausted(&mut sim, attacker));
}
```

- [x] Extend the suspended replacement matrix with nested replacement. Use the exact
      production effect below as the outer weapon's program, followed by `weapon_pause()`:

```rust
weapon_replacement_effect()
```

This existing helper installs an ordinary weapon through Hero replacement. Give both old and
outer weapons Windfury, spend one attack before playing the outer weapon, and verify that the
new current Hero is exhausted while suspended and after `weapon_restore_and_finish`. Resolve
its ID with `hero(&mut sim, PlayerId::One)` after replacement. The active weapon must be neither
old nor outer; both superseded IDs must end in Graveyard exactly once. This covers original
source removal without erroneously restoring its grant.

- [x] Run Gazelle immediately, then `--test_filter=weapon_windfury`. Expected: PASS, including
      JSON restoration and fork comparisons through the existing restore helper.

## Task 4: Document and verify the slice

**Modify:** the three simulator Markdown documents and approved spec listed in the file map.

- [x] Add the following behavior paragraph to README and revise the older blanket weapon-keyword
      deferral to say that Lifesteal, Poisonous, and other weapon keyword mechanics remain deferred:

```text
Active weapon Windfury contributes to the Hero's allowance of two attacks during its
controller's turn. Attacks already spent count across weapon changes, and Hero-owned and
weapon-owned Windfury do not stack. Current weapon keywords determine the contribution;
replacement and removal update allowance without changing attack history. Validation,
legal actions, snapshot exhaustion, and Frozen thawing use the same calculation.
```

- [x] Add this compatibility/policy text to RULEBOOK_CONFORMANCE and link the approved spec:

```text
Weapon Windfury uses the existing active-weapon lifetime and controller-turn policy.
Pending destruction contributes until existing removal; temporary grants are observed by
thaw before expiration. Accepted attacks retain existing continuation guards. These timing
choices are engine policies, not certified pinned-rulebook conformance. Schema 17 remains
structurally compatible; replay equivalence with older binaries where weapon Windfury was
inactive is not guaranteed.
```

- [x] Mark only the weapon-Windfury slice implemented in IMPLEMENTATION_PROGRESS; keep
      Milestone 8 incomplete. Update the spec status to implemented after verification succeeds.
      Record actual test counts and commands from the following runs rather than forecasting them.

```bash
nix develop --command aspect format --scope=all
nix develop --command aspect test //hearthstone_simulator/...
nix develop --command aspect build //...
nix develop --command aspect lint //hearthstone_simulator/...
git diff --check
```

- [x] If formatting changes source, run Gazelle immediately afterward and inspect generated
      differences. Rerun affected checks only when edits or failures justify it.
- [x] Review the diff: no keyword copying, new state, schema bump, generic inheritance, or
      unrelated changes. Verify every acceptance row in the spec maps to a passing test.
- [x] Commit only the feature files and updated docs after required checks pass:

```bash
git add hearthstone_simulator/simulator/weapon.rs hearthstone_simulator/simulator/simulation_action_validation.rs hearthstone_simulator/simulator/simulation_tests_actions.rs hearthstone_simulator/README.md hearthstone_simulator/IMPLEMENTATION_PROGRESS.md hearthstone_simulator/RULEBOOK_CONFORMANCE.md docs/superpowers/specs/2026-09-16-hearthstone-weapon-windfury-design.md
git commit -m 'feat(hearthstone): support weapon-granted Windfury'
```

## Plan self-review

The tasks cover live allowance, history and non-stacking, weapon lifecycle, keyword changes,
zero Attack, off-turn behavior, thaw ordering, accepted attacks, replacement overlap, nested
replacement, and restored continuation. Existing Minion and equipment suites remain regression
coverage. All new API usage is restricted to helpers already in the action tests and the one
new query explicitly defined in Task 1. No additional dependencies or source-file splits are
required for this slice.
