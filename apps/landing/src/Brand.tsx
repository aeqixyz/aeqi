import { useRef, useEffect, useCallback } from "react";
import Nav from "./Nav";
import Footer from "./Footer";

function drawParticles(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  text: string,
  fontSize: number,
  particleCount: number,
  particleSize: number
) {
  // Sample glyph
  const tmp = document.createElement("canvas");
  tmp.width = w;
  tmp.height = h;
  const tc = tmp.getContext("2d")!;
  tc.fillStyle = "#000";
  tc.textAlign = "center";
  tc.textBaseline = "middle";
  tc.font = `bold ${fontSize}px Inter, -apple-system, system-ui, sans-serif`;
  tc.fillText(text, w / 2, h / 2);

  const imageData = tc.getImageData(0, 0, w, h);
  const filled: [number, number][] = [];
  for (let y = 0; y < h; y += 1) {
    for (let x = 0; x < w; x += 1) {
      if (imageData.data[(y * w + x) * 4 + 3] > 128) {
        filled.push([x, y]);
      }
    }
  }

  // Draw background
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, w, h);

  // Draw particles
  for (let i = 0; i < particleCount; i++) {
    const pt = filled[Math.floor(Math.random() * filled.length)];
    if (!pt) continue;
    const x = pt[0] + (Math.random() - 0.5) * 2;
    const y = pt[1] + (Math.random() - 0.5) * 2;
    const size = particleSize * (0.5 + Math.random());
    const opacity = 0.15 + Math.random() * 0.55;

    ctx.beginPath();
    ctx.arc(x, y, size, 0, Math.PI * 2);
    ctx.fillStyle = `rgba(0, 0, 0, ${opacity})`;
    ctx.fill();
  }
}

function BrandCanvas({
  id,
  width,
  height,
  text,
  fontSize,
  particles,
  particleSize,
  label,
  dimensions,
}: {
  id: string;
  width: number;
  height: number;
  text: string;
  fontSize: number;
  particles: number;
  particleSize: number;
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

    // Wait for font
    if (document.fonts?.ready) {
      document.fonts.ready.then(() => {
        drawParticles(ctx, width, height, text, fontSize, particles, particleSize);
      });
    } else {
      setTimeout(() => {
        drawParticles(ctx, width, height, text, fontSize, particles, particleSize);
      }, 200);
    }
  }, [width, height, text, fontSize, particles, particleSize]);

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
              fontSize={220}
              particles={8000}
              particleSize={2.5}
              label="LinkedIn Banner"
              dimensions="1584 × 396px"
            />

            <BrandCanvas
              id="company-logo"
              width={400}
              height={400}
              text="æqi"
              fontSize={140}
              particles={4000}
              particleSize={2}
              label="Company Logo"
              dimensions="400 × 400px"
            />

            <BrandCanvas
              id="icon"
              width={512}
              height={512}
              text="æ"
              fontSize={380}
              particles={5000}
              particleSize={2.5}
              label="Icon / Favicon"
              dimensions="512 × 512px"
            />

            <BrandCanvas
              id="og-image"
              width={1200}
              height={630}
              text="æqi"
              fontSize={280}
              particles={10000}
              particleSize={2.5}
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
