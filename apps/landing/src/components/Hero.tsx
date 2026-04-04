import { motion } from "framer-motion";

const primitives = [
  { text: "Agent", color: "#c0392b", direction: 1 },
  { text: "Event", color: "#c0392b", direction: -1 },
  { text: "Quest", color: "#c0392b", direction: 1 },
  { text: "Insight", color: "#c0392b", direction: -1 },
];

export function Hero() {
  return (
    <section className="relative min-h-screen flex flex-col items-center justify-center overflow-hidden">
      <style>{`
        @keyframes drift-right {
          0%, 100% { transform: translateX(0px); }
          50% { transform: translateX(40px); }
        }
        @keyframes drift-left {
          0%, 100% { transform: translateX(0px); }
          50% { transform: translateX(-40px); }
        }
      `}</style>

      {/* Ambient glow orb */}
      <div
        className="absolute w-[600px] h-[600px] rounded-full pointer-events-none"
        style={{
          background: "radial-gradient(circle, rgba(192,57,43,0.06) 0%, rgba(192,57,43,0.02) 40%, transparent 70%)",
          animation: "pulse-ambient 6s ease-in-out infinite",
        }}
      />

      <motion.h1
        className="leading-[1.05] text-left select-none relative z-10"
        style={{ fontFamily: "'Space Grotesk', sans-serif" }}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 1, ease: "easeOut" }}
      >
        {primitives.map((w, i) => (
          <motion.span
            className="block whitespace-nowrap text-5xl md:text-7xl lg:text-[88px] font-bold tracking-tight"
            key={i}
            initial={{ opacity: 0, x: w.direction * -120 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{
              duration: 1.1,
              ease: [0.16, 1, 0.3, 1],
              delay: 0.1 + i * 0.12,
            }}
            style={{
              animation: `${w.direction > 0 ? "drift-right" : "drift-left"} ${6 + i * 1.5}s ease-in-out infinite`,
              animationDelay: `${1.5 + i * 0.4}s`,
            }}
          >
            <span style={{ color: w.color, textShadow: `0 0 30px ${w.color}40` }}>
              {w.text[0]}
            </span>
            <span className="text-white/40">{w.text.slice(1)}</span>
          </motion.span>
        ))}
      </motion.h1>

      {/* Tagline */}
      <motion.p
        className="mt-10 text-[15px] md:text-[17px] text-white/45 tracking-wide text-center relative z-10"
        style={{ fontFamily: "'Inter', sans-serif" }}
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.8, delay: 1.0, ease: "easeOut" }}
      >
        Unopinionated <span className="text-white/90 font-semibold">artificial intelligence</span> orchestration.
      </motion.p>

      {/* Scroll indicator */}
      <motion.div
        className="absolute bottom-12 left-1/2 -translate-x-1/2"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 2, duration: 1 }}
      >
        <div className="w-[1px] h-8 bg-gradient-to-b from-white/0 via-white/10 to-white/0 animate-pulse-slow" />
      </motion.div>
    </section>
  );
}
