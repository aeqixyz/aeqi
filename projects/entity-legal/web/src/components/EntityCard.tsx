"use client";

import { useRef, useState, useCallback } from "react";
import { motion } from "framer-motion";
import { transitions, fadeIn } from "@/lib/animations";
import { useReducedMotion } from "@/lib/hooks";

export function EntityCard() {
  const cardRef = useRef<HTMLDivElement>(null);
  const reduced = useReducedMotion();
  const [rotate, setRotate] = useState({ x: 0, y: 0 });
  const [hovering, setHovering] = useState(false);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (reduced) return;
      const card = cardRef.current;
      if (!card) return;
      const rect = card.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const centerX = rect.width / 2;
      const centerY = rect.height / 2;
      setRotate({
        x: ((y - centerY) / centerY) * -8,
        y: ((x - centerX) / centerX) * 8,
      });
    },
    [reduced]
  );

  const handleMouseLeave = useCallback(() => {
    setRotate({ x: 0, y: 0 });
    setHovering(false);
  }, []);

  return (
    <motion.div
      initial={reduced ? false : fadeIn.initial}
      animate={fadeIn.animate}
      transition={{ ...transitions.slow, delay: 0.5 }}
      className="mt-10"
      style={{ perspective: "1000px" }}
    >
      <div
        ref={cardRef}
        onMouseMove={handleMouseMove}
        onMouseEnter={() => setHovering(true)}
        onMouseLeave={handleMouseLeave}
        className="mx-auto w-full max-w-[380px]"
        style={{
          transform: `rotateX(${rotate.x}deg) rotateY(${rotate.y}deg)`,
          transition: hovering ? "transform 0.1s ease-out" : "transform 0.4s ease-out",
          transformStyle: "preserve-3d",
        }}
      >
        {/* The card */}
        <div className="relative overflow-hidden rounded-xl border border-border bg-bg-card p-7 shadow-2xl shadow-black/40">
          {/* Subtle shine effect */}
          <div
            className="pointer-events-none absolute inset-0 opacity-[0.04]"
            style={{
              background: hovering
                ? `radial-gradient(circle at ${50 + rotate.y * 4}% ${50 + rotate.x * 4}%, white 0%, transparent 60%)`
                : "none",
            }}
          />

          {/* Card content */}
          <div className="relative">
            {/* Top row */}
            <div className="flex items-start justify-between">
              <span className="font-serif text-[15px] tracking-[0.12em] text-text-secondary">
                entity<span className="text-text-tertiary">.</span>legal
              </span>
              <span className="text-[11px] uppercase tracking-[0.15em] text-text-tertiary">
                Series LLC
              </span>
            </div>

            {/* Entity name */}
            <div className="mt-6">
              <p className="text-[11px] uppercase tracking-[0.15em] text-text-tertiary">
                Entity
              </p>
              <p className="mt-1 text-[17px] font-medium text-text-primary">
                Acme Autonomous Agent
              </p>
            </div>

            {/* Details row */}
            <div className="mt-5 flex gap-8">
              <div>
                <p className="text-[11px] uppercase tracking-[0.15em] text-text-tertiary">
                  Series
                </p>
                <p className="mt-1 text-[14px] tabular-nums text-text-secondary">
                  #0047
                </p>
              </div>
              <div>
                <p className="text-[11px] uppercase tracking-[0.15em] text-text-tertiary">
                  Jurisdiction
                </p>
                <p className="mt-1 text-[14px] text-text-secondary">
                  Marshall Islands
                </p>
              </div>
              <div>
                <p className="text-[11px] uppercase tracking-[0.15em] text-text-tertiary">
                  Status
                </p>
                <p className="mt-1 text-[14px] text-text-secondary">
                  Active
                </p>
              </div>
            </div>

            {/* Bottom row */}
            <div className="mt-5 flex items-end justify-between border-t border-border pt-4">
              <p className="text-[12px] tabular-nums text-text-tertiary">
                EL-2026-0047
              </p>
              <p className="text-[12px] tabular-nums text-text-tertiary">
                Est. 2026
              </p>
            </div>
          </div>
        </div>
      </div>
    </motion.div>
  );
}
