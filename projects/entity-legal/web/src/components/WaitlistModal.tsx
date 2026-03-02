"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X } from "lucide-react";
import { transitions } from "@/lib/animations";
import { track } from "@/lib/track";

interface WaitlistModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function WaitlistModal({ isOpen, onClose }: WaitlistModalProps) {
  const [formState, setFormState] = useState<
    "form" | "submitting" | "success"
  >("form");
  const [email, setEmail] = useState("");
  const [honeypot, setHoneypot] = useState("");
  const formOpenedAt = useRef(0);
  const emailTouched = useRef(false);

  const closeWith = useCallback((method: string) => {
    track("waitlist_close", { method });
    onClose();
  }, [onClose]);

  useEffect(() => {
    if (isOpen) {
      formOpenedAt.current = Date.now();
      emailTouched.current = false;
      setFormState("form");
      setEmail("");
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [isOpen]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeWith("escape");
    };
    if (isOpen) {
      window.addEventListener("keydown", handler);
      return () => window.removeEventListener("keydown", handler);
    }
  }, [isOpen, closeWith]);

  const handleEmailFocus = useCallback(() => {
    if (!emailTouched.current) {
      emailTouched.current = true;
      track("waitlist_input");
    }
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (honeypot) return;
    if (Date.now() - formOpenedAt.current < 2000) return;

    setFormState("submitting");
    track("waitlist_submit");

    try {
      const res = await fetch("/api/waitlist", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, website: honeypot }),
      });
      if (!res.ok) throw new Error();
      setFormState("success");
      track("waitlist_success");
    } catch {
      setFormState("form");
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 bg-[#09090B]/80 backdrop-blur-sm"
            onClick={() => closeWith("backdrop")}
            aria-hidden="true"
          />

          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 flex items-center justify-center p-4"
            role="dialog"
            aria-modal="true"
            aria-label="Join the waitlist"
          >
            <div
              className="relative w-full max-w-[420px] rounded-lg border border-border bg-bg-card p-8"
              onClick={(e) => e.stopPropagation()}
            >
              <button
                onClick={() => closeWith("x")}
                className="absolute right-4 top-4 p-1 text-text-tertiary transition-colors hover:text-text-primary"
                aria-label="Close"
              >
                <X className="h-4 w-4" />
              </button>

              {formState === "success" ? (
                <div className="text-center">
                  <h2 className="font-serif text-2xl text-text-primary">
                    You&rsquo;re on the list.
                  </h2>
                  <p className="mt-3 text-sm leading-relaxed text-text-secondary">
                    We&rsquo;ll reach out when formation opens.
                  </p>
                </div>
              ) : (
                <>
                  <h2 className="font-serif text-2xl text-text-primary">
                    Get notified when we launch.
                  </h2>

                  <form onSubmit={handleSubmit} className="mt-6">
                    {/* Honeypot */}
                    <div
                      className="absolute -left-[9999px]"
                      aria-hidden="true"
                    >
                      <label htmlFor="website">Website</label>
                      <input
                        type="text"
                        id="website"
                        name="website"
                        tabIndex={-1}
                        autoComplete="off"
                        value={honeypot}
                        onChange={(e) => setHoneypot(e.target.value)}
                      />
                    </div>

                    <div>
                      <label
                        htmlFor="waitlist-email"
                        className="mb-2 block text-sm text-text-secondary"
                      >
                        Email address
                      </label>
                      <input
                        type="email"
                        id="waitlist-email"
                        value={email}
                        onChange={(e) => setEmail(e.target.value)}
                        onFocus={handleEmailFocus}
                        placeholder="you@example.com"
                        required
                        className="w-full rounded-lg border border-border bg-bg-primary px-4 py-3 text-sm text-text-primary placeholder:text-text-tertiary focus:border-border-hover focus:outline-none"
                      />
                    </div>

                    <button
                      type="submit"
                      disabled={formState === "submitting"}
                      className="mt-4 w-full rounded-lg border border-border py-3 text-sm font-medium text-text-primary transition-colors hover:border-border-hover hover:bg-bg-primary disabled:opacity-50"
                    >
                      {formState === "submitting"
                        ? "Submitting..."
                        : "Notify Me"}
                    </button>
                  </form>
                </>
              )}
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
