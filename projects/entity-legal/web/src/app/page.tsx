"use client";

import { useState } from "react";
import { Hero } from "@/components/Hero";
import { WaitlistModal } from "@/components/WaitlistModal";
import { config } from "@/lib/config";

export default function Home() {
  const [waitlistOpen, setWaitlistOpen] = useState(false);

  const openWaitlist = () => {
    if (config.waitlistMode) {
      setWaitlistOpen(true);
    }
  };

  return (
    <>
      <Hero onCtaClick={openWaitlist} />
      <WaitlistModal
        isOpen={waitlistOpen}
        onClose={() => setWaitlistOpen(false)}
      />
    </>
  );
}
