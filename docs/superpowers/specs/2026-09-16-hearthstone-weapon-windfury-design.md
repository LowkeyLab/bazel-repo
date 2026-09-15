# Weapon-granted Windfury

## Status and scope

Design approved on 2026-09-16. Implementation is pending written-spec review.
This Milestone 8 slice makes the active weapon's Windfury contribute to Hero attack allowance.
Lifesteal, Poisonous, arbitrary weapon-keyword transfer, Mega-Windfury, and new combat phases remain outside this slice.

## Existing foundation

At origin/main commit 8ac9c232, weapons support play, scoped replacement, live Hero Attack,
durability, Deathrattles, and checkpoint-exact continuation. Character Windfury already derives
an allowance of two attacks from current keywords while preserving attacks spent.

- `hearthstone_simulator/simulator/weapon.rs`: `active` identifies the authoritative equipped weapon; `attack_weapon` restricts contribution to Heroes during their controller's turn.
- `hearthstone_simulator/simulator/simulation_action_validation.rs`: `attack_exhausted` shares readiness and allowance semantics with snapshots and `can_thaw`.
- `hearthstone_simulator/simulator/aura.rs`: `has_keyword` reads current entity keywords and supported aura contributions.
- `hearthstone_simulator/simulator/simulation_snapshot.rs`: object exhaustion uses the shared exhaustion query.
- `hearthstone_simulator/core/checkpoint.rs`: schema 17 already stores weapon equipment, keywords, enchantments, and attacks spent.

## Alternatives and decision

Use a live weapon contribution query. Copying a weapon-owned Windfury grant onto the Hero
would require attachment ownership, cleanup, and synchronization on replacement and keyword
changes. A live query reuses equipment authority and avoids additional durable state.

Keep ordinary entity keyword lookup local to that entity. Introduce a narrowly named weapon
Windfury query in `weapon.rs` that resolves `attack_weapon` and reads the selected weapon's
current Windfury. The shared attack-allowance calculation combines that result with the
character's own Windfury using logical OR. Do not generalize `has_keyword` into automatic
weapon-keyword inheritance.

## Behavior contract

1. A character with its own Windfury has an allowance of two attacks. A Hero also has that
   allowance when its active weapon currently has Windfury and it is the controller's turn.
   Otherwise the allowance remains one. Multiple sources do not stack.
2. Readiness blocking, Frozen, positive Attack, ownership, target restrictions, and all other
   declaration rules remain independent. Equipping a weapon never refreshes readiness or
   resets attacks spent.
3. All attacks already spent count, including attacks with another weapon or no weapon.
   Gaining Windfury after one attack allows one more; gaining it after two allows none.
4. Weapon contribution uses only the authoritative active ID. Superseded weapons still
   observing replacement events cannot contribute. Nested equip completion cannot restore
   an older weapon's contribution.
5. Replacing, removing, destroying, or removing Windfury from the active weapon changes the
   next allowance query immediately. Independent Hero Windfury remains effective.
6. Native and ordinary enchantment-granted weapon Windfury use the weapon's current keywords.
   Existing silence, backward-movement, copying, and enchantment-expiration policies apply.
   This feature adds no weapon transformation support or special lifecycle reset.
7. Follow existing equipment lifetime: zero durability or pending destruction does not itself
   remove a still-active in-Play weapon's contribution before the existing removal boundary.
   At completed action boundaries, a broken weapon no longer contributes.
8. Weapon Attack value does not gate Windfury contribution. A zero-Attack Windfury weapon can
   increase allowance when independent Hero Attack makes attacks possible.
9. Off-turn weapon contribution is absent, matching the existing `attack_weapon` policy.
   Hero-owned Windfury continues to follow its existing semantics.

For example: attack once with an ordinary weapon, then equip a Windfury weapon. The Hero
can attack once more. Replacing it with another Windfury weapon after that second attack
cannot grant a third attack.

## Integration and timing

Validation, canonical legal-action enumeration, snapshot exhaustion, and Frozen thawing must
all observe the same allowance through `attack_exhausted`. Enumeration remains read-only.
Snapshot exhaustion still describes readiness and allowance, not complete target availability.

The existing thaw step runs before temporary enchantment expiration. A temporary weapon
Windfury grant therefore contributes at that step if it is still present. Preserve existing
end-turn ordering; no additional thaw pass is introduced.

An accepted attack is not revalidated for Windfury after Attack reactions. Removing or replacing
the weapon during those reactions affects subsequent declarations, while the accepted attack
continues subject to existing surviving-participant and outcome guards. Damage preparation,
captured durability payment, attack usage, AfterAttack, and death boundaries keep their
existing order.

## Errors and persistence

Use existing validation errors for exhausted or otherwise invalid attacks. The contribution
query returns false when the equipment lookup finds no eligible active weapon; existing
checkpoint and invariant validation remain responsible for rejecting corrupt equipment.

No new component, resolution operation, snapshot field, or serialized field is needed. Keep
checkpoint schema 17. Restored state recomputes allowance using the updated behavior.
Restoration and fork within the updated engine must produce identical checkpoints and traces.
Checkpoints remain structurally compatible, but behavior is not promised to match older
binaries where weapon Windfury was inactive; document that distinction.

## Acceptance tests

Add focused behavioral fixtures using the existing synthetic weapon and attack helpers:

- A Windfury weapon permits two Hero attacks, charges durability for each completed attack,
  and rejects a third. Check legal actions and snapshot exhaustion at each boundary.
- Equip after one attack with another weapon and after an unarmed attack backed by Hero
  Attack. Preserve attacks spent and permit exactly one additional attack.
- Replace Windfury with ordinary and Windfury weapons, regain Windfury, and verify independent
  Hero Windfury and non-stacking sources.
- Break a one-durability weapon after the first attack; retain independent Hero Attack so
  rejection proves loss of allowance rather than absence of Attack.
- Add, remove, and expire weapon Windfury through ordinary keyword enchantments. Verify that
  unrelated weapon keywords do not transfer.
- Verify zero-Attack weapon contribution with independent Hero Attack, off-turn exclusion,
  ordinary turn refresh, and unchanged Minion Windfury behavior.
- Verify Frozen thawing after one attack with active weapon Windfury, without it, and with
  a grant expiring at the existing post-thaw expiration step.
- Suspend replacement while old and new weapons coexist; verify only the active weapon
  contributes, including nested replacement and original-weapon removal.
- Remove weapon Windfury during an accepted second attack's reactions; preserve accepted
  continuation and apply current allowance to subsequent declarations.
- Restore JSON checkpoints and fork at suspended attack/replacement boundaries; compare final
  checkpoints and traces. Repeated legal-action enumeration must leave state and trace unchanged.

## Implementation verification and documentation

After source edits run `bazel run //:gazelle` immediately and before formatting. Run
`aspect format --scope=all`, `aspect test //hearthstone_simulator/...`, `aspect build //...`,
and scoped simulator lint. Record actual results. Update README, IMPLEMENTATION_PROGRESS,
and RULEBOOK_CONFORMANCE when implementation lands, leaving broader Milestone 8 work open.

## Evidence and policy limits

The current [Windfury reference](https://hearthstone.wiki.gg/wiki/Windfury) describes counting
previous Hero attacks even when performed with a different weapon or no weapon. This supports
the attack-history contract. It does not certify all timing decisions against the repository's
pinned advanced-rulebook revision 913067.

Off-turn contribution, replacement overlap, pending-destruction lifetime, thaw ordering, and
accepted-attack continuation are explicit extensions of existing engine policies. Do not label
those details verified pinned-rulebook conformance without additional evidence.
