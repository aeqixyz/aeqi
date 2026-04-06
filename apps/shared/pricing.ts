/** Central pricing config. Imported by both landing page and dashboard app. */

export const TRIAL = {
  days: 7,
  companies: 1,
  agents: 3,
  tokens: "5M",
};

export const PLANS = [
  {
    id: "starter" as const,
    name: "Starter",
    price: 29,
    popular: false,
    tagline: "Launch your first autonomous company.",
    desc: "For individuals getting started with autonomous agents.",
    features: [
      "3 companies",
      "10 agents",
      "25M LLM tokens / month",
      "On-chain cap table",
      "Economy listing",
      "Bring your own LLM key",
    ],
    short: [
      "3 companies",
      "10 agents",
      "25M tokens / month",
      "Email support",
    ],
  },
  {
    id: "growth" as const,
    name: "Growth",
    price: 79,
    popular: true,
    tagline: "Run a portfolio at scale.",
    desc: "For teams running multiple companies with higher volume.",
    features: [
      "Everything in Starter",
      "15 companies",
      "50 agents",
      "150M LLM tokens / month",
      "Priority support",
      "Custom agent templates",
    ],
    short: [
      "15 companies",
      "50 agents",
      "150M tokens / month",
      "Priority support",
      "Custom agent templates",
    ],
  },
] as const;

export type PlanId = (typeof PLANS)[number]["id"];
