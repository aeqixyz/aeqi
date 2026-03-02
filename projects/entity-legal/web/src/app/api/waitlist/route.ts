import { NextRequest, NextResponse } from "next/server";
import { createClient } from "redis";

const REDIS_URL = process.env.REDIS_URL || "redis://127.0.0.1:6379";
const RATE_LIMIT_TTL = 3600; // 1 hour

let redis: ReturnType<typeof createClient> | null = null;

async function getRedis() {
  if (!redis || !redis.isOpen) {
    redis = createClient({ url: REDIS_URL });
    await redis.connect();
  }
  return redis;
}

export async function POST(req: NextRequest) {
  try {
    const { email, website } = await req.json();

    // Honeypot check
    if (website) {
      return NextResponse.json({ ok: true });
    }

    // Email validation
    if (!email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      return NextResponse.json({ error: "Invalid email" }, { status: 400 });
    }

    const normalizedEmail = email.toLowerCase().trim();

    // Rate limit by IP
    const ip = req.headers.get("x-forwarded-for")?.split(",")[0]?.trim() || "unknown";
    const client = await getRedis();
    const rateKey = `entity:rate:${ip}`;
    const existing = await client.get(rateKey);
    if (existing) {
      // Silent success to prevent enumeration
      return NextResponse.json({ ok: true });
    }
    await client.set(rateKey, "1", { EX: RATE_LIMIT_TTL });

    // Store email
    await client.sAdd("entity:waitlist", normalizedEmail);

    return NextResponse.json({ ok: true });
  } catch {
    return NextResponse.json({ error: "Server error" }, { status: 500 });
  }
}
