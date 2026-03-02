import { NextRequest, NextResponse } from "next/server";

const DISCOUNT_CODE = "CURL-ENTITY-42";
const DISCOUNT_PCT = 10;

export async function POST(req: NextRequest) {
  return NextResponse.json(
    {
      status: "queued",
      entity_id: null,
      jurisdiction: "Marshall Islands",
      structure: "Series DAO LLC",
      message: "Formation is not yet live. You found the API early.",
      discount: {
        code: DISCOUNT_CODE,
        percent_off: DISCOUNT_PCT,
        note: `Use code ${DISCOUNT_CODE} at launch for ${DISCOUNT_PCT}% off your first entity.`,
      },
      docs: "https://api.entity.legal",
      _: "You curl, we respect that.",
    },
    { status: 202 }
  );
}

export async function GET() {
  return NextResponse.json(
    {
      api: "entity.legal",
      version: "v1",
      status: "coming soon",
      endpoints: {
        "POST /v1/incorporate": "Form a new entity",
        "GET /v1/entities/:id": "Retrieve entity details",
        "GET /v1/entities/:id/shares": "Cap table",
      },
      hint: "Try: curl -X POST https://api.entity.legal/v1/incorporate",
    },
    { status: 200 }
  );
}
