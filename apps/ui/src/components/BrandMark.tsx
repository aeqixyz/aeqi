/**
 * BrandMark — the æqi wordmark with the shifted "i".
 * Use this everywhere the brand name appears to keep it consistent.
 *
 * The "i" is shifted down by 0.04em to match the landing page brand.
 * Accepts size prop for font-size and className for additional styling.
 */
export default function BrandMark({
  size = 18,
  className = "",
  color,
}: {
  size?: number;
  className?: string;
  color?: string;
}) {
  return (
    <span
      className={className}
      style={{
        fontFamily: "var(--font-sans)",
        fontSize: size,
        fontWeight: 700,
        letterSpacing: "-0.08em",
        color: color || "rgba(0, 0, 0, 0.85)",
        lineHeight: 1,
      }}
    >
      æq<span style={{ display: "inline-block", transform: "translateY(0.04em)" }}>i</span>
    </span>
  );
}
