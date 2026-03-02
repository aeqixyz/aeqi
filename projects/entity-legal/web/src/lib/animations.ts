export const transitions = {
  default: { duration: 0.3, ease: "easeOut" as const },
  slow: { duration: 0.6, ease: "easeOut" as const },
};

export const fadeIn = {
  initial: { opacity: 0 },
  animate: { opacity: 1 },
};
