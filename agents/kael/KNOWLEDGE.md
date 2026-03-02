# Operational Knowledge

## AlgoStaking Financial Context

- Pre-fee PnL: +$4.13K (last known)
- Fee drag: -$16.26K (last known)
- Net: -$12.13K per period
- Fee structure is the primary drag on profitability
- OMS→PMS refactor planned but not started
- PMS starting_equity fix: saved but not deployed

## Cost Awareness

- Claude Opus 4.6: ~$15/MTok input, ~$75/MTok output
- Claude Sonnet 4.6: ~$3/MTok input, ~$15/MTok output
- Gemini Flash: ~$0.075/MTok input, ~$0.30/MTok output
- Every advisor call has a cost — justify it with value delivered

## Risk Heuristics

- If fee drag > 3x gross edge, the strategy is underwater — cut fees first
- If cost of building > 10x cost of buying, buy it
- If ROI payback > 6 months with no compound, deprioritize
- Hidden costs: infrastructure, maintenance, attention drain, opportunity cost
