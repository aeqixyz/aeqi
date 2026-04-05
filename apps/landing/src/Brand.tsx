import { useRef, useEffect, useCallback } from "react";
import Nav from "./Nav";
import Footer from "./Footer";

function drawText(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  text: string,
  fontSize: number,
  align: "center" | "right" = "center",
  offsetX: number = 0
) {
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, w, h);

  const x = align === "right" ? w * 0.7 + offsetX : w / 2 + offsetX;
  ctx.fillStyle = "rgba(0, 0, 0, 0.5)";
  ctx.textAlign = align === "right" ? "center" : "center";
  ctx.textBaseline = "middle";
  ctx.font = `bold ${fontSize}px Inter, -apple-system, system-ui, sans-serif`;
  ctx.fillText(text, x, h / 2);
}

function BrandCanvas({
  id,
  width,
  height,
  text,
  fontSize,
  align = "center",
  offsetX = 0,
  label,
  dimensions,
}: {
  id: string;
  width: number;
  height: number;
  text: string;
  fontSize: number;
  align?: "center" | "right";
  offsetX?: number;
  label: string;
  dimensions: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d")!;

    if (document.fonts?.ready) {
      document.fonts.ready.then(() => {
        drawText(ctx, width, height, text, fontSize, align, offsetX);
      });
    } else {
      setTimeout(() => {
        drawText(ctx, width, height, text, fontSize, align, offsetX);
      }, 200);
    }
  }, [width, height, text, fontSize, align, offsetX]);

  const download = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const link = document.createElement("a");
    link.download = `aeqi-${id}.png`;
    link.href = canvas.toDataURL("image/png");
    link.click();
  }, [id]);

  return (
    <div className="mb-16">
      <div className="flex items-baseline justify-between mb-4">
        <div>
          <h3 className="text-[17px] font-semibold text-black/85">{label}</h3>
          <p className="text-[14px] text-black/45 mt-1">{dimensions}</p>
        </div>
        <button
          onClick={download}
          className="text-[14px] text-black/60 hover:text-black/85 transition-colors underline underline-offset-4 decoration-black/20 hover:decoration-black/40 cursor-pointer"
        >
          Download PNG
        </button>
      </div>
      <canvas
        ref={canvasRef}
        className="w-full rounded-xl border border-black/[0.06]"
        style={{ maxWidth: width, aspectRatio: `${width}/${height}` }}
      />
    </div>
  );
}

export default function Brand() {
  return (
    <div className="min-h-screen flex flex-col bg-white">
      <Nav />
      <section className="flex-1 px-6 pt-28 pb-20">
        <div className="max-w-3xl mx-auto">
          <h1 className="text-[28px] font-semibold tracking-tight text-black/85">Brand</h1>
          <p className="mt-2 text-[16px] text-black/50">Download assets for press, social, and integrations.</p>

          <div className="mt-16">
            <BrandCanvas
              id="linkedin-banner"
              width={1584}
              height={396}
              text="æqi"
              fontSize={180}
              align="right"
              label="LinkedIn Banner"
              dimensions="1584 × 396px — text right-aligned, space for profile photo on the left"
            />

            <BrandCanvas
              id="company-logo"
              width={400}
              height={400}
              text="æqi"
              fontSize={120}
              label="Company Logo"
              dimensions="400 × 400px"
            />

            <BrandCanvas
              id="icon"
              width={512}
              height={512}
              text="æ"
              fontSize={340}
              label="Icon / Favicon"
              dimensions="512 × 512px"
            />

            <BrandCanvas
              id="og-image"
              width={1200}
              height={630}
              text="æqi"
              fontSize={240}
              label="Open Graph Image"
              dimensions="1200 × 630px"
            />
          </div>
        </div>
      </section>
      <div className="bg-[#fafafa]">
        <Footer />
      </div>
    </div>
  );
}
