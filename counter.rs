//! SimplePearScrow – Stylus port (template‑only, unaudited)

#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

// Efficient WASM allocator
#[global_allocator]
static ALLOC: mini_alloc::MiniAlloc = mini_alloc::MiniAlloc::INIT;

// ─── Imports ────────────────────────────────────────────────────────────────
use alloy_sol_types::sol;   // `sol!` for ABI/event types
use stylus_sdk::{
    alloy_primitives::{address, Address, U256},
    evm,             // emit events
    msg,             // msg::sender()
    block,           // block::timestamp()
    prelude::*,
};

// ─── Constants ──────────────────────────────────────────────────────────────
const AVS: Address = address!("7b01d9f5338f348ab7a90af84f797c0ea51c7a44"); // no 0x prefix
const FIXED_AMOUNT_RAW: u64 = 10_000_000; // 10 tokens (6‑decimals)

#[inline(always)]
fn fixed_amount() -> U256 {
    U256::from(FIXED_AMOUNT_RAW)
}

// ─── Minimal ERC‑20 interface ───────────────────────────────────────────────
sol_interface! {
    interface IERC20 {
        function transfer(address to, uint256 value) external returns (bool);
    }
}

// ─── Storage layout (parallel mappings) ─────────────────────────────────────
sol_storage! {
    #[entrypoint]
    pub struct SimplePearScrow {
        address fee_collector;            // set lazily on first order
        uint256 next_order_id;

        mapping(uint256 => address) order_token;
        mapping(uint256 => address) order_buyer;
        mapping(uint256 => uint256) order_amount;
        mapping(uint256 => bool)    order_released;
        mapping(uint256 => uint256) order_created_at;

        mapping(address => uint256) collected_fees; // reserved for future use
    }
}

// ─── Event ABI ───────────────────────────────────────────────────────────────
sol! {
    event OrderCreated(
        uint256 indexed orderId,
        address  indexed buyer,
        address         token,
        uint256         amount
    );
}

// ─── Public API ──────────────────────────────────────────────────────────────
#[external]
impl SimplePearScrow {
    /* ── View helpers ──────────────────────────────────────────────────── */
    pub fn fee_collector(&self) -> Address { self.fee_collector.get() }
    pub fn next_order_id(&self) -> U256     { self.next_order_id.get() }

    /* ── Buyer creates an order ───────────────────────────────────────── */
    pub fn create_order(&mut self, token: Address) {
        assert!(token != Address::ZERO, "invalid token");

        // first caller becomes fee collector
        if self.fee_collector.get() == Address::ZERO {
            self.fee_collector.set(msg::sender());
        }

        let order_id = self.next_order_id.get();
        let now      = U256::from(block::timestamp());

        // write fields
        self.order_token.insert(order_id, token);
        self.order_buyer.insert(order_id, msg::sender());
        self.order_amount.insert(order_id, fixed_amount());
        self.order_released.insert(order_id, false);
        self.order_created_at.insert(order_id, now);

        self.next_order_id.set(order_id + U256::from(1));

        // emit event
        evm::log(OrderCreated {
            orderId: order_id,
            buyer:   msg::sender(),
            token,
            amount:  fixed_amount(),
        });
    }

    /* ── AVS releases escrow ──────────────────────────────────────────── */
    pub fn release_order(&mut self, order_id: U256, result: bool) {
        //assert!(msg::sender() == AVS, "unauthorised");

        assert!(self.order_amount.get(order_id) > U256::ZERO, "invalid order");
        assert!(!self.order_released.get(order_id), "already released");
        assert!(result, "order not approved");

        self.order_released.insert(order_id, true);

        let token = self.order_token.get(order_id);
        let buyer = self.order_buyer.get(order_id);

        // call ERC‑20 transfer: context is `&mut self`
        let ok = IERC20::new(token)
            .transfer(self, buyer, fixed_amount())
            .expect("low‑level call failed");

        assert!(ok, "token transfer returned false");
    }
}
