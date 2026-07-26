#!/usr/bin/env python3
"""Integration test for the real Noir poker circuits.

This test executes the actual `deal_valid`, `reveal_board_valid`, and
`showdown_valid` circuits in sequence, using the public outputs of each step as
inputs to the next step. It verifies that:

1. `deal_valid` commits a deterministic dealt deck and hand commitments.
2. `reveal_board_valid` reveals non-overlapping board cards from that same deck.
3. `showdown_valid` consumes the deal/reveal outputs and proves the correct
   winner for the resulting 7-card hands.

Prerequisites:
- `nargo` 1.0.0-beta.17 available on PATH or in `NARGO_BIN`
- `bb` (Barretenberg CLI) available on PATH or in `BB_BIN`
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
import os
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CIRCUITS_DIR = REPO_ROOT / "circuits"
MAX_PLAYERS = 6
MAX_REVEAL = 3
MAX_USED = 16
IDENTITY_PERMUTATION = list(range(52))


def resolve_binary(name: str, env_var: str) -> str:
    configured = os.environ.get(env_var)
    if configured:
        return configured
    found = shutil.which(name)
    if found:
        return found
    raise unittest.SkipTest(
        f"{name} is required for this test. Install it or set {env_var}."
    )


def run(cmd: list[str], cwd: Path, env: dict[str, str] | None = None) -> None:
    subprocess.run(cmd, cwd=cwd, env=env, check=True)


def write_prover_toml(path: Path, values: dict[str, object]) -> None:
    lines: list[str] = []
    for key, value in values.items():
        lines.append(f"{key} = {toml_literal(value)}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def toml_literal(value: object) -> str:
    if isinstance(value, list):
        inner = ", ".join(toml_literal(item) for item in value)
        return f"[{inner}]"
    if isinstance(value, int):
        return f'"{value}"'
    raise TypeError(f"unsupported TOML value: {value!r}")


def parse_public_inputs(public_inputs_path: Path) -> list[int]:
    raw = public_inputs_path.read_bytes()
    assert len(raw) % 32 == 0, "public_inputs must be 32-byte aligned"
    return [int.from_bytes(raw[i : i + 32], "big") for i in range(0, len(raw), 32)]


def flatten_bb_output(path: Path) -> Path:
    if path.is_dir():
        candidate = path / path.name
        if candidate.exists():
            shutil.move(candidate, path.with_suffix(".tmp"))
            shutil.rmtree(path)
            path.with_suffix(".tmp").replace(path)
    return path


def apply_permutation(deck: list[int], permutation: list[int]) -> list[int]:
    return [deck[index] for index in permutation]


def derive_shared_deck(fixture: dict[str, list[int]]) -> list[int]:
    canonical = list(range(52))
    after_party0 = apply_permutation(canonical, fixture["party0_permutation"])
    after_party1 = apply_permutation(after_party0, fixture["party1_permutation"])
    return apply_permutation(after_party1, fixture["party2_permutation"])


def assert_valid_deck(deck: list[int]) -> None:
    assert sorted(deck) == list(range(52))


def score_five(cards: list[int]) -> int:
    ranks = sorted((card % 13 for card in cards), reverse=True)
    suits = [card // 13 for card in cards]

    is_flush = len(set(suits)) == 1
    is_straight = all(ranks[i] == ranks[i + 1] + 1 for i in range(4))
    is_wheel = ranks == [12, 3, 2, 1, 0]

    eq0 = ranks[0] == ranks[1]
    eq1 = ranks[1] == ranks[2]
    eq2 = ranks[2] == ranks[3]
    eq3 = ranks[3] == ranks[4]

    has_four = (eq0 and eq1 and eq2) or (eq1 and eq2 and eq3)
    four_rank = ranks[0] if (eq0 and eq1 and eq2) else ranks[4]

    has_three = (eq0 and eq1) or (eq1 and eq2) or (eq2 and eq3)
    if eq0 and eq1:
        three_rank = ranks[0]
    elif eq1 and eq2:
        three_rank = ranks[1]
    else:
        three_rank = ranks[2]

    is_full_house = ((eq0 and eq1) and eq3) or ((eq2 and eq3) and eq0)
    full_house_pair_rank = ranks[4] if (eq0 and eq1) else ranks[0]

    has_two_pairs = ((eq0 and eq2) or (eq0 and eq3) or (eq1 and eq3)) and not has_four
    pair_rank_hi = ranks[0] if eq0 else ranks[1]
    pair_rank_lo = ranks[4] if eq3 else ranks[2]

    has_pair = (eq0 or eq1 or eq2 or eq3) and not has_three and not has_two_pairs and not has_four
    if eq0:
        one_pair_rank = ranks[0]
    elif eq1:
        one_pair_rank = ranks[1]
    elif eq2:
        one_pair_rank = ranks[2]
    else:
        one_pair_rank = ranks[3]

    tb = (
        (ranks[0] << 16)
        | (ranks[1] << 12)
        | (ranks[2] << 8)
        | (ranks[3] << 4)
        | ranks[4]
    )

    if is_flush and is_straight and ranks[0] == 12:
        return (9 << 20) | tb
    if is_flush and (is_straight or is_wheel):
        high = 3 << 16 if is_wheel else tb
        return (8 << 20) | high
    if has_four:
        return (7 << 20) | (four_rank << 16)
    if is_full_house:
        return (6 << 20) | (three_rank << 8) | full_house_pair_rank
    if is_flush:
        return (5 << 20) | tb
    if is_straight or is_wheel:
        high = 3 << 16 if is_wheel else ranks[0] << 16
        return (4 << 20) | high
    if has_three:
        return (3 << 20) | (three_rank << 16)
    if has_two_pairs:
        return (2 << 20) | (pair_rank_hi << 12) | (pair_rank_lo << 8)
    if has_pair:
        return (1 << 20) | (one_pair_rank << 16)
    return tb


def evaluate_hand(cards: list[int]) -> int:
    best = 0
    for skip1 in range(7):
        for skip2 in range(skip1 + 1, 7):
            hand = [cards[i] for i in range(7) if i not in (skip1, skip2)]
            best = max(best, score_five(hand))
    return best


class CircuitSequenceIntegrationTest(unittest.TestCase):
    maxDiff = None

    @classmethod
    def setUpClass(cls) -> None:
        cls.nargo = resolve_binary("nargo", "NARGO_BIN")
        cls.bb = resolve_binary("bb", "BB_BIN")

    def test_real_circuits_run_in_sequence_and_stay_consistent(self) -> None:
        fixture = self.build_fixture()
        expected_deck = derive_shared_deck(fixture)
        assert_valid_deck(expected_deck)

        with tempfile.TemporaryDirectory(prefix="circuit-sequence-", dir=REPO_ROOT) as tmpdir:
            workspace = Path(tmpdir)

            deal_publics = self.run_circuit(
                workspace,
                "deal_valid",
                {
                    "num_players": 3,
                    "party0_permutation": fixture["party0_permutation"],
                    "party0_salts": fixture["party0_salts"],
                    "party1_permutation": fixture["party1_permutation"],
                    "party1_salts": fixture["party1_salts"],
                    "party2_permutation": fixture["party2_permutation"],
                    "party2_salts": fixture["party2_salts"],
                },
            )
            self.assertEqual(len(deal_publics), 20)
            self.assertEqual(deal_publics[0], 3)

            deck_root = deal_publics[1]
            hand_commitments = deal_publics[2:8]
            dealt_card1_indices = deal_publics[8:14]
            dealt_card2_indices = deal_publics[14:20]
            self.assertEqual(dealt_card1_indices[:3], [0, 2, 4])
            self.assertEqual(dealt_card2_indices[:3], [1, 3, 5])

            used_indices = [0, 1, 2, 3, 4, 5]
            flop_publics = self.run_circuit(
                workspace,
                "reveal_board_valid",
                {
                    "deck_root": deck_root,
                    "num_revealed": 3,
                    "num_previously_used": len(used_indices),
                    "previously_used_indices": used_indices + [0] * (MAX_USED - len(used_indices)),
                    "party0_permutation": fixture["party0_permutation"],
                    "party0_salts": fixture["party0_salts"],
                    "party1_permutation": fixture["party1_permutation"],
                    "party1_salts": fixture["party1_salts"],
                    "party2_permutation": fixture["party2_permutation"],
                    "party2_salts": fixture["party2_salts"],
                },
            )
            self.assertEqual(len(flop_publics), 25)
            self.assertEqual(flop_publics[0], deck_root)
            self.assertEqual(flop_publics[1], 3)
            self.assertEqual(flop_publics[2], 6)
            self.assertEqual(flop_publics[22:25], [6, 7, 8])
            self.assertEqual(flop_publics[19:22], [expected_deck[6], expected_deck[7], expected_deck[8]])

            used_indices.extend(flop_publics[22:25])
            turn_publics = self.run_circuit(
                workspace,
                "reveal_board_valid",
                {
                    "deck_root": deck_root,
                    "num_revealed": 1,
                    "num_previously_used": len(used_indices),
                    "previously_used_indices": used_indices + [0] * (MAX_USED - len(used_indices)),
                    "party0_permutation": fixture["party0_permutation"],
                    "party0_salts": fixture["party0_salts"],
                    "party1_permutation": fixture["party1_permutation"],
                    "party1_salts": fixture["party1_salts"],
                    "party2_permutation": fixture["party2_permutation"],
                    "party2_salts": fixture["party2_salts"],
                },
            )
            self.assertEqual(turn_publics[22], 9)
            self.assertEqual(turn_publics[19], expected_deck[9])

            used_indices.append(turn_publics[22])
            river_publics = self.run_circuit(
                workspace,
                "reveal_board_valid",
                {
                    "deck_root": deck_root,
                    "num_revealed": 1,
                    "num_previously_used": len(used_indices),
                    "previously_used_indices": used_indices + [0] * (MAX_USED - len(used_indices)),
                    "party0_permutation": fixture["party0_permutation"],
                    "party0_salts": fixture["party0_salts"],
                    "party1_permutation": fixture["party1_permutation"],
                    "party1_salts": fixture["party1_salts"],
                    "party2_permutation": fixture["party2_permutation"],
                    "party2_salts": fixture["party2_salts"],
                },
            )
            self.assertEqual(river_publics[22], 10)
            self.assertEqual(river_publics[19], expected_deck[10])

            board_indices = [6, 7, 8, 9, 10]
            showdown_publics = self.run_circuit(
                workspace,
                "showdown_valid",
                {
                    "num_active_players": 3,
                    "hand_commitments": hand_commitments,
                    "board_indices": board_indices,
                    "deck_root": deck_root,
                    "party0_permutation": fixture["party0_permutation"],
                    "party0_salts": fixture["party0_salts"],
                    "party1_permutation": fixture["party1_permutation"],
                    "party1_salts": fixture["party1_salts"],
                    "party2_permutation": fixture["party2_permutation"],
                    "party2_salts": fixture["party2_salts"],
                },
            )
            self.assertEqual(len(showdown_publics), 27)
            self.assertEqual(showdown_publics[0], 3)
            self.assertEqual(showdown_publics[1:7], hand_commitments)
            self.assertEqual(showdown_publics[7:12], board_indices)
            self.assertEqual(showdown_publics[12], deck_root)

            hole_card1 = showdown_publics[13:19]
            hole_card2 = showdown_publics[19:25]
            winner_index = showdown_publics[25]
            tie_mask = showdown_publics[26]

            self.assertEqual(hole_card1[:3], [expected_deck[0], expected_deck[2], expected_deck[4]])
            self.assertEqual(hole_card2[:3], [expected_deck[1], expected_deck[3], expected_deck[5]])

            board_cards = [
                flop_publics[19],
                flop_publics[20],
                flop_publics[21],
                turn_publics[19],
                river_publics[19],
            ]
            self.assertEqual(board_cards, [expected_deck[i] for i in board_indices])

            scores = []
            for player in range(3):
                cards = [
                    hole_card1[player],
                    hole_card2[player],
                    *board_cards,
                ]
                scores.append(evaluate_hand(cards))

            expected_winner = max(range(3), key=lambda seat: scores[seat])
            best_score = scores[expected_winner]
            expected_tie_mask = sum(1 << seat for seat, score in enumerate(scores) if score == best_score)

            self.assertEqual(winner_index, expected_winner)
            self.assertEqual(tie_mask, expected_tie_mask)
            self.assertEqual(winner_index, 1)
            self.assertEqual(tie_mask, 0b010)

    def run_circuit(self, workspace: Path, circuit_name: str, prover_values: dict[str, object]) -> list[int]:
        self.prepare_workspace(workspace, circuit_name)
        circuit_dir = workspace / "circuits" / circuit_name
        write_prover_toml(circuit_dir / "Prover.toml", prover_values)

        run([self.nargo, "compile", "--program-dir", str(circuit_dir)], cwd=workspace)
        run([self.nargo, "execute", "--program-dir", str(circuit_dir)], cwd=workspace)

        acir_path = circuit_dir / "target" / f"{circuit_name}.json"
        witness_path = circuit_dir / "target" / f"{circuit_name}.gz"
        target_dir = circuit_dir / "target"
        run(
            [
                self.bb,
                "prove",
                "--scheme",
                "ultra_honk",
                "--oracle_hash",
                "keccak",
                "--bytecode_path",
                str(acir_path),
                "--witness_path",
                str(witness_path),
                "--output_format",
                "bytes_and_fields",
                "--output_path",
                str(target_dir),
            ],
            cwd=workspace,
        )

        public_inputs_path = flatten_bb_output(target_dir / "public_inputs")
        return parse_public_inputs(public_inputs_path)

    def prepare_workspace(self, workspace: Path, circuit_name: str) -> None:
        circuit_root = workspace / "circuits"
        if circuit_root.exists():
            shutil.rmtree(circuit_root)
        shutil.copytree(CIRCUITS_DIR / "lib", circuit_root / "lib")
        shutil.copytree(CIRCUITS_DIR / circuit_name, circuit_root / circuit_name)

    def build_fixture(self) -> dict[str, list[int]]:
        opening_cards = [0, 14, 12, 25, 11, 24, 4, 19, 34, 49, 8]
        used = set(opening_cards)
        party0_permutation = opening_cards + [card for card in range(52) if card not in used]

        return {
            "party0_permutation": party0_permutation,
            "party1_permutation": IDENTITY_PERMUTATION.copy(),
            "party2_permutation": IDENTITY_PERMUTATION.copy(),
            "party0_salts": [1_000 + i for i in range(52)],
            "party1_salts": [2_000 + (i * 3) for i in range(52)],
            "party2_salts": [3_000 + (i * 5) for i in range(52)],
        }


if __name__ == "__main__":
    unittest.main()
