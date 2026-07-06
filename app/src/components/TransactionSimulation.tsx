"use client";

import { useState } from "react";
import type { SimulationResult, StateChange } from "@/lib/transaction-simulation";

interface TransactionSimulationProps {
  simulation: SimulationResult;
  onConfirm: () => void;
  onCancel: () => void;
  loading?: boolean;
}

function StateChangeIcon({ type }: { type: StateChange['type'] }) {
  const iconMap = {
    contract_data: "📋",
    balance: "💰", 
    trustline: "🔗",
    account: "👤",
  };
  return <span className="mr-2">{iconMap[type]}</span>;
}

function SimulationStatus({ success, error }: { success: boolean; error?: string }) {
  if (success) {
    return (
      <div className="flex items-center text-green-600 mb-4">
        <span className="mr-2">✅</span>
        <span>Transaction simulation successful</span>
      </div>
    );
  }

  return (
    <div className="mb-4">
      <div className="flex items-center text-red-600 mb-2">
        <span className="mr-2">❌</span>
        <span>Transaction simulation failed</span>
      </div>
      {error && (
        <div className="text-sm text-red-500 bg-red-50 p-2 rounded">
          {error}
        </div>
      )}
    </div>
  );
}

export function TransactionSimulation({
  simulation,
  onConfirm,
  onCancel,
  loading = false,
}: TransactionSimulationProps) {
  const [showDetails, setShowDetails] = useState(false);

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg p-6 max-w-md w-full mx-4 max-h-[80vh] overflow-y-auto">
        <h2 className="text-xl font-bold mb-4">Transaction Preview</h2>
        
        <SimulationStatus success={simulation.success} error={simulation.error} />

        {simulation.success && (
          <>
            <div className="mb-4">
              <h3 className="font-semibold text-gray-700 mb-2">Cost</h3>
              <div className="text-sm">
                <div>Fee: {simulation.fee} XLM</div>
                {simulation.gasUsed && (
                  <div>Gas Used: {simulation.gasUsed.toLocaleString()} instructions</div>
                )}
              </div>
            </div>

            <div className="mb-4">
              <h3 className="font-semibold text-gray-700 mb-2">Expected Changes</h3>
              <div className="space-y-1">
                {simulation.stateChanges.map((change, index) => (
                  <div key={index} className="text-sm flex items-start">
                    <StateChangeIcon type={change.type} />
                    <span>{change.description}</span>
                  </div>
                ))}
              </div>
            </div>

            {simulation.rawResult && (
              <button
                onClick={() => setShowDetails(!showDetails)}
                className="text-xs text-blue-600 hover:text-blue-800 mb-4"
              >
                {showDetails ? "Hide" : "Show"} technical details
              </button>
            )}

            {showDetails && simulation.rawResult && (
              <div className="mb-4 text-xs bg-gray-50 p-2 rounded overflow-auto max-h-32">
                <pre>{JSON.stringify(simulation.rawResult, null, 2)}</pre>
              </div>
            )}
          </>
        )}

        <div className="flex gap-2 pt-4 border-t">
          <button
            onClick={onCancel}
            className="flex-1 px-4 py-2 border border-gray-300 rounded hover:bg-gray-50"
            disabled={loading}
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={!simulation.success || loading}
            className="flex-1 px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:bg-gray-300 disabled:cursor-not-allowed"
          >
            {loading ? "Signing..." : "Sign & Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
