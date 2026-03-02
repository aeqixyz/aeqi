# RiftDecks Project Knowledge

Scarcity-driven TCG drop shop. Weekly drops of 10 tournament-winning Riftbound decks sourced from Chinese market at 80% of English TCGPlayer prices. Limited stock per deck.

## Architecture

```
Next.js 16 (App Router) → Stripe Checkout (dynamic pricing) → Redis (inventory)
                        → Plausible CE (analytics)
                        → TCGCSV API (live TCGPlayer prices)
```

- Runtime: Next.js production (`next start`) on port 3200, systemd, nginx reverse proxy + SSL
- Payments: Stripe LIVE mode, dynamic `price_data` (no fixed Products)
- Inventory: Redis DECR/INCR (atomic reserve/release)
- Analytics: Plausible CE v2.1 (Docker) at analytics.riftdecks.shop
- Pricing: 80% of live TCGPlayer market via tcgcsv.com

## Key Files

| File | Purpose |
|------|---------|
| `src/app/page.tsx` | Landing page (hero, champion select, FAQ) |
| `src/app/deck/[id]/page.tsx` | Deck detail + checkout (MAIN REVENUE PAGE) |
| `src/app/success/page.tsx` | Post-purchase confirmation |
| `src/app/api/checkout/route.ts` | Stripe session + inventory reserve |
| `src/app/api/webhook/route.ts` | Stripe events (completed/expired) |
| `src/app/api/inventory/route.ts` | Current stock per deck |
| `src/app/api/card-prices/route.ts` | Per-card live prices from TCGCSV |
| `src/app/api/all-prices/route.ts` | Bulk prices for landing page |
| `src/lib/redis.ts` | Inventory management (reserve/release/get) |
| `src/lib/decks.ts` | Deck definitions, addon config, pricing |
| `src/lib/tcgcsv.ts` | TCGCSV API client |
| `src/lib/stripe.ts` | Stripe client init |
| `src/lib/track.ts` | Plausible event wrapper |
| `src/lib/riftbound-index.json` | Card name → TCGCSV product ID mapping |
| `src/components/ChampionSelect.tsx` | Deck grid (2 rows of 5) |
| `src/components/CountdownTimer.tsx` | Drop countdown |
| `src/components/InventoryBar.tsx` | Stock visualization |

## Product Structure

**Tournament Deck** (Section 1 — radio):
- Core Edition: 64 cards, sleeved, Chinese translations
- Strategy Edition: 100 cards (64 + 36 tech), sleeved

**Rarity Upgrades** (Section 2 — independent checkboxes):
- Full Foil, Alternate Art, Overnumbered

**Extras** (Section 3):
- Vault ($29): deck box, double sleeves, sideboard in toploaders, legend in magnetic case
- Loot ($79): stitched mat, 20 dice, 100 sleeves, 100 inners, 50 toploaders, 5 magnetic cases

## Pricing Logic

```
Card price = 80% of TCGPlayer market total (with selected upgrades)
Per card: highest selected upgrade price wins (foil vs alt art vs overnumbered)
Total = card price + vault (if selected) + loot (if selected)
```

Server-side in `/api/checkout/route.ts` recomputes — NEVER trust client.

## Inventory

Redis keys: `inventory:{deckId}` (e.g. `inventory:kaisa`)

Flow:
1. User clicks LOCK IN → `/api/checkout` calls `reserveUnit()` (DECR)
2. If DECR below 0 → INCR back, return SOLD OUT
3. Stripe session created with 30-min expiry
4. On `checkout.session.completed` → sale final
5. On `checkout.session.expired` or `async_payment_failed` → `releaseUnit()` (INCR)

```bash
# Check inventory
redis-cli MGET inventory:annie inventory:azir inventory:draven inventory:fiora inventory:irelia inventory:jax inventory:kaisa inventory:lucian inventory:sett inventory:viktor

# Reset all to 15
for deck in annie azir draven fiora irelia jax kaisa lucian sett viktor; do redis-cli SET "inventory:$deck" 15; done
```

## Drop Lifecycle

1. Set countdown: `NEXT_PUBLIC_DROP_END` in `.env.local`
2. Seed inventory via redis-cli
3. Go live: rebuild + restart
4. Monitor: analytics.riftdecks.shop + Stripe dashboard
5. End drop: set all inventory to 0

## Analytics (Plausible)

Dashboard: https://analytics.riftdecks.shop

Funnel events:
| Event | Where | Props |
|-------|-------|-------|
| `landing_scroll` | Landing | section |
| `time_to_click` | Landing | seconds |
| `deck_click` | Landing | deck, price |
| `deck_view` | Deck page | deck |
| `checkout_click` | Deck page | deck, total, foil, altArt, overnumbered, vault, loot |
| `purchase_complete` | Success | session |

Plausible config: `/opt/plausible/` (Docker), nginx proxy, script.js proxied through Next.js rewrites (adblocker bypass).

## Stripe

- Mode: LIVE (sk_live_...)
- Webhook: https://riftdecks.shop/api/webhook
- Events: completed, expired, async_payment_succeeded, async_payment_failed
- Session expiry: 30 minutes
- Keys in `.env.local` (gitignored)

## Infrastructure

| Service | URL | Port |
|---------|-----|------|
| Next.js | https://riftdecks.shop | 3200 |
| Plausible | https://analytics.riftdecks.shop | 8000 |
| Redis | localhost | 6379 |

Deploy: merge to master → post-merge hook → `next build` + `systemctl restart riftdecks`

## CN Card API

- Official API: `POST lol-api.playloltcg.com/xcx/card/searchCardCraftWeb` (710 cards, no auth)
- CDN images: `cdn.playloltcg.com` (public PNGs, convert to WebP)
- Set mapping: TCGPlayer SFG = CN SFD (Spiritforged)
- TCGPlayer puts `*` in signature card numbers — don't double-transform
- Build: `node tools/build-cn-index.mjs [--download]`
- Index: `src/data/cn-card-index.json` (710 entries, keyed by `SET:number`)

## Adding a New Deck

1. Add entry to `DECKS` in `src/lib/decks.ts`
2. Add deck ID to `DECK_KEYS` in `src/lib/redis.ts`
3. Update inventory defaults in `src/app/page.tsx`
4. Map card names in `src/lib/riftbound-index.json`
5. Seed: `redis-cli SET inventory:newdeck 15`

## Adding a New Drop

1. Update `NEXT_PUBLIC_DROP_END` in `.env.local`
2. Swap deck entries for new meta
3. Reset inventory
4. Update drop badge number in `src/app/page.tsx`
5. Rebuild: `npx next build && sudo systemctl restart riftdecks`
