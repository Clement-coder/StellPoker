"use client";

import { useState } from "react";
import type { SimulationResult } from "@/lib/transaction-simulation";
import type { WalletSession } from "@/lib/wallet";

export interface TransactionFlow {
  showSimulation: boolean;
  simulation: SimulationResult | null;
  loading: boolean;
  error: string | null;
}

export function useTransactionSimulation() {
  const [flow, setFlow] = useState<TransactionFlow>({
    showSimulation: false,
    simulation: null,
    loading: false,
    error: null,
  });

  const startSimulation = async (
    simulateAsync: () => Promise<SimulationResult>
  ) => {
    setFlow(prev => ({ ...prev, loading: true, error: null }));
    
    try {
      const simulation = await simulateAsync();
      setFlow({
        showSimulation: true,
        simulation,
        loading: false,
        error: null,
      });
    } catch (error) {
      setFlow({
        showSimulation: false,
        simulation: null,
        loading: false,
        error: error instanceof Error ? error.message : "Simulation failed",
      });
    }
  };

  const executeTransaction = async (
    executeAsync: () => Promise<void>
  ) => {
    setFlow(prev => ({ ...prev, loading: true }));
    
    try {
      await executeAsync();
      setFlow({
        showSimulation: false,
        simulation: null,
        loading: false,
        error: null,
      });
    } catch (error) {
      setFlow(prev => ({
        ...prev,
        loading: false,
        error: error instanceof Error ? error.message : "Transaction failed",
      }));
    }
  };

  const cancelSimulation = () => {
    setFlow({
      showSimulation: false,
      simulation: null,
      loading: false,
      error: null,
    });
  };

  const clearError = () => {
    setFlow(prev => ({ ...prev, error: null }));
  };

  return {
    ...flow,
    startSimulation,
    executeTransaction,
    cancelSimulation,
    clearError,
  };
}

// Convenience functions for common transaction types
export function useJoinTableSimulation(
  wallet: WalletSession | null,
  onSuccess?: () => void
) {
  const simulation = useTransactionSimulation();

  const simulateJoinTable = async (tableId: number, buyIn: bigint) => {
    if (!wallet) throw new Error("Wallet not connected");
    
    const { simulateJoinTable } = await import("@/lib/transaction-simulation");
    return simulateJoinTable(wallet.address, tableId, buyIn);
  };

  const executeJoinTable = async (tableId: number, buyIn: bigint) => {
    if (!wallet) throw new Error("Wallet not connected");
    
    const { joinTableOnChain } = await import("@/lib/onchain");
    await joinTableOnChain(wallet, tableId, buyIn);
    onSuccess?.();
  };

  const joinTable = (tableId: number, buyIn: bigint) => {
    simulation.startSimulation(() => simulateJoinTable(tableId, buyIn));
  };

  const confirmJoin = (tableId: number, buyIn: bigint) => {
    simulation.executeTransaction(() => executeJoinTable(tableId, buyIn));
  };

  return {
    ...simulation,
    joinTable,
    confirmJoin,
  };
}

export function usePlayerActionSimulation(
  wallet: WalletSession | null,
  onSuccess?: () => void
) {
  const simulation = useTransactionSimulation();

  const simulatePlayerAction = async (
    tableId: number,
    action: string,
    amount?: number
  ) => {
    if (!wallet) throw new Error("Wallet not connected");
    
    const { simulatePlayerAction } = await import("@/lib/transaction-simulation");
    return simulatePlayerAction(wallet.address, tableId, action as any, amount);
  };

  const executePlayerAction = async (
    tableId: number,
    action: string,
    amount?: number
  ) => {
    if (!wallet) throw new Error("Wallet not connected");
    
    const { playerActionOnChain } = await import("@/lib/onchain");
    await playerActionOnChain(wallet, tableId, action as any, amount);
    onSuccess?.();
  };

  const performAction = (tableId: number, action: string, amount?: number) => {
    simulation.startSimulation(() => simulatePlayerAction(tableId, action, amount));
  };

  const confirmAction = (tableId: number, action: string, amount?: number) => {
    simulation.executeTransaction(() => executePlayerAction(tableId, action, amount));
  };

  return {
    ...simulation,
    performAction,
    confirmAction,
  };
}
