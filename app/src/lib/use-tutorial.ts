"use client";

import { useState, useEffect, useCallback } from "react";

const TUTORIAL_STORAGE_KEY = "stellpoker-tutorial-seen";

export type TutorialStep =
  | "welcome"
  | "table-layout"
  | "betting-actions"
  | "chip-tray"
  | "proof-explorer"
  | "wallet-connection"
  | "done";

export interface TutorialState {
  isOpen: boolean;
  currentStep: TutorialStep;
  hasSeen: boolean;
}

const STEP_ORDER: TutorialStep[] = [
  "welcome",
  "table-layout",
  "betting-actions",
  "chip-tray",
  "proof-explorer",
  "wallet-connection",
  "done",
];

export function useTutorial() {
  const [isOpen, setIsOpen] = useState(false);
  const [currentStep, setCurrentStep] = useState<TutorialStep>("welcome");
  const [hasSeen, setHasSeen] = useState(true); // default true to avoid flash

  // Read from localStorage after mount to decide whether to auto-show
  useEffect(() => {
    const seen = localStorage.getItem(TUTORIAL_STORAGE_KEY) === "true";
    setHasSeen(seen);
    if (!seen) {
      // Small delay so the table renders first
      const timer = setTimeout(() => setIsOpen(true), 800);
      return () => clearTimeout(timer);
    }
  }, []);

  const open = useCallback(() => {
    setCurrentStep("welcome");
    setIsOpen(true);
  }, []);

  const close = useCallback(() => {
    setIsOpen(false);
    setHasSeen(true);
    localStorage.setItem(TUTORIAL_STORAGE_KEY, "true");
  }, []);

  const next = useCallback(() => {
    setCurrentStep((prev) => {
      const idx = STEP_ORDER.indexOf(prev);
      if (idx < STEP_ORDER.length - 1) return STEP_ORDER[idx + 1];
      return prev;
    });
  }, []);

  const prev = useCallback(() => {
    setCurrentStep((prev) => {
      const idx = STEP_ORDER.indexOf(prev);
      if (idx > 0) return STEP_ORDER[idx - 1];
      return prev;
    });
  }, []);

  const goTo = useCallback((step: TutorialStep) => {
    setCurrentStep(step);
  }, []);

  const currentIndex = STEP_ORDER.indexOf(currentStep);
  const totalSteps = STEP_ORDER.length - 1; // exclude "done" from count display
  const isLastStep = currentStep === "done";
  const isFirstStep = currentStep === "welcome";

  return {
    isOpen,
    currentStep,
    currentIndex,
    totalSteps,
    isLastStep,
    isFirstStep,
    hasSeen,
    open,
    close,
    next,
    prev,
    goTo,
  };
}
