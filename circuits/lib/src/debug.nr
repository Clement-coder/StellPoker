// Circuit-level debugging utilities (compile-time controlled)
//
// All debug output is removed in release builds via dead code elimination.
// Enable with: nargo build --debug
// Disable with: nargo build --release (default)

pub global DEBUG_ENABLED: bool = false;

// Debug output for a single field value (name-value pair)
pub fn debug_field(label: &str, value: Field) {
    if DEBUG_ENABLED {
        std::println(f"{label} = {value}");
    }
}

// Debug output for a single u32 value
pub fn debug_u32(label: &str, value: u32) {
    if DEBUG_ENABLED {
        std::println(f"{label} = {value}");
    }
}

// Debug output for a card value with human-readable format
// Card encoding: suit * 13 + rank
// suit: 0=Clubs, 1=Diamonds, 2=Hearts, 3=Spades
// rank: 0=2, 1=3, ..., 8=10, 9=J, 10=Q, 11=K, 12=A
pub fn debug_card(card: Field) {
    if DEBUG_ENABLED {
        let card_u32 = card as u32;
        let suit = card_u32 / 13;
        let rank = card_u32 % 13;
        let suit_name = if suit == 0 {
            "♣"
        } else if suit == 1 {
            "♦"
        } else if suit == 2 {
            "♥"
        } else {
            "♠"
        };
        let rank_name = if rank == 0 {
            "2"
        } else if rank == 1 {
            "3"
        } else if rank == 2 {
            "4"
        } else if rank == 3 {
            "5"
        } else if rank == 4 {
            "6"
        } else if rank == 5 {
            "7"
        } else if rank == 6 {
            "8"
        } else if rank == 7 {
            "9"
        } else if rank == 8 {
            "10"
        } else if rank == 9 {
            "J"
        } else if rank == 10 {
            "Q"
        } else if rank == 11 {
            "K"
        } else {
            "A"
        };
        std::println(f"Card: {rank_name}{suit_name} (value={card_u32})");
    }
}

// Debug output for array of fields
pub fn debug_array_fields<let N: u32>(label: &str, arr: [Field; N]) {
    if DEBUG_ENABLED {
        std::println(f"{label}: [");
        for i in 0..N {
            std::println(f"  {i}: {arr[i]}");
        }
        std::println("]");
    }
}

// Debug output for array of u32 values
pub fn debug_array_u32<let N: u32>(label: &str, arr: [u32; N]) {
    if DEBUG_ENABLED {
        std::println(f"{label}: [");
        for i in 0..N {
            std::println(f"  {i}: {arr[i]}");
        }
        std::println("]");
    }
}

// Debug output for assertion results (helps identify which assertion failed)
pub fn debug_assertion(condition: bool, label: &str) {
    if DEBUG_ENABLED {
        if condition {
            std::println(f"✓ {label}");
        } else {
            std::println(f"✗ {label}");
        }
    }
}
