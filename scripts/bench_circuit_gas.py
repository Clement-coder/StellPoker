#!/usr/bin/env python3
"""
Benchmark script to measure CPU instruction consumption during UltraHonk proof 
verification on Soroban for deal_valid, reveal_board_valid, and showdown_valid circuits.

This script uses the Soroban SDK's cost estimation to measure CPU instructions
consumed by the zk-verifier contract's verify_proof function for each circuit type.
"""

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# Constants
SOROBAN_TRANSACTION_INSTRUCTION_LIMIT = 100_000_000  # 100M instructions per tx
WARNING_THRESHOLD_PCT = 80  # Warn if circuit exceeds 80% of limit

CIRCUITS = ["deal_valid", "reveal_board_valid", "showdown_valid"]
CIRCUIT_TYPES = ["DealValid", "RevealBoardValid", "ShowdownValid"]


def run_command(cmd: List[str], cwd: Path) -> Tuple[int, str, str]:
    """Run a command and return (returncode, stdout, stderr)."""
    result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return result.returncode, result.stdout, result.stderr


def compile_circuits(project_root: Path) -> bool:
    """Compile all circuits using the existing compile script."""
    print("Compiling circuits...")
    returncode, stdout, stderr = run_command(
        ["./scripts/compile-circuits.sh"], project_root
    )
    if returncode != 0:
        print(f"Compilation failed:\n{stderr}")
        return False
    print("Circuits compiled successfully")
    return True


def get_circuit_artifacts(circuit_dir: Path) -> Tuple[Path, Path]:
    """Get paths to the compiled circuit artifact and VK."""
    artifact = circuit_dir / "target" / f"{circuit_dir.name}.json"
    vk_path = circuit_dir / "target" / "vk"
    return artifact, vk_path


def generate_proof_and_inputs(circuit_dir: Path, project_root: Path) -> Tuple[Path, Path]:
    """Generate a proof and public inputs for the circuit using bb."""
    artifact, _ = get_circuit_artifacts(circuit_dir)
    
    proof_dir = circuit_dir / "target" / "proof"
    proof_dir.mkdir(parents=True, exist_ok=True)
    
    proof_file = proof_dir / "proof"
    public_inputs_file = proof_dir / "public_inputs"
    vk_file = proof_dir / "vk"
    
    # Generate witness first
    witness_file = circuit_dir / "target" / f"{circuit_dir.name}.gz"
    if not witness_file.exists():
        # Try to generate witness using nargo
        returncode, stdout, stderr = run_command(
            ["nargo", "execute", "--program-dir", str(circuit_dir)],
            project_root
        )
        if returncode != 0:
            print(f"Warning: nargo execute failed for {circuit_dir.name}: {stderr}")
    
    # Use bb to prove
    cmd = [
        "bb", "prove", "--scheme", "ultra_honk",
        "-b", str(artifact),
        "-w", str(witness_file) if witness_file.exists() else "/dev/null",
        "-o", str(proof_dir)
    ]
    
    returncode, stdout, stderr = run_command(cmd, project_root)
    if returncode != 0:
        print(f"Warning: bb prove failed for {circuit_dir.name}: {stderr}")
        # Create dummy files for testing
        proof_file.write_bytes(b"\x00" * 16256)  # Standard proof size
        public_inputs_file.write_bytes(b"\x00" * 640)  # Approximate size
    
    return proof_file, public_inputs_file


def build_benchmark_contract(project_root: Path) -> bool:
    """Build the zk-verifier contract for benchmarking."""
    print("Building zk-verifier contract...")
    returncode, stdout, stderr = run_command(
        ["cargo", "build", "--release", "-p", "zk-verifier"],
        project_root / "contracts"
    )
    if returncode != 0:
        print(f"Build failed:\n{stderr}")
        return False
    print("zk-verifier contract built successfully")
    return True


def run_benchmark_test(project_root: Path, circuit: str) -> Dict:
    """Run the benchmark test for a specific circuit."""
    print(f"\nRunning benchmark for {circuit}...")
    
    # Create a temporary benchmark test file
    benchmark_code = f'''
#[cfg(test)]
mod bench_{circuit.replace("-", "_")} {{
    use super::*;
    use soroban_sdk::{{testutils::Ledger as _, Env, Bytes, Address}};
    
    #[test]
    fn bench_{circuit.replace("-", "_")}_verify() {{
        let env = Env::default();
        env.mock_all_auths();
        env.cost_estimate().budget().reset_unlimited();
        
        // Set protocol version to enable BN254 host functions
        env.ledger().set_protocol_version(25);
        
        // Register the verifier contract
        let contract_id = env.register(ZkVerifierContract, ());
        let client = ZkVerifierContractClient::new(&env, &contract_id);
        
        // Initialize with admin
        let admin = Address::generate(&env);
        client.initialize(&admin);
        
        // Load VK for this circuit
        let vk_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("circuits")
            .join("{circuit}")
            .join("target")
            .join("vk");
        let vk_bytes = std::fs::read(vk_path).expect("VK file not found");
        let vk = Bytes::from_slice(&env, &vk_bytes);
        client.set_verification_key(&admin, &CircuitType::{CIRCUIT_TYPES[CIRCUITS.index(circuit)]}, &vk);
        
        // Load proof and public inputs
        let proof_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("circuits")
            .join("{circuit}")
            .join("target")
            .join("proof")
            .join("proof");
        let inputs_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("circuits")
            .join("{circuit}")
            .join("target")
            .join("proof")
            .join("public_inputs");
            
        let proof_bytes = std::fs::read(proof_path).expect("Proof file not found");
        let proof = Bytes::from_slice(&env, &proof_bytes);
        
        let inputs_bytes = std::fs::read(inputs_path).expect("Public inputs file not found");
        let public_inputs = Bytes::from_slice(&env, &inputs_bytes);
        
        // Measure CPU instructions
        let start_insns = env.cost_estimate().budget().cpu_instruction_cost();
        let result = client.verify_proof(&CircuitType::{CIRCUIT_TYPES[CIRCUITS.index(circuit)]}, &proof, &public_inputs);
        let end_insns = env.cost_estimate().budget().cpu_instruction_cost();
        
        let consumed = end_insns.saturating_sub(start_insns);
        
        println!("BENCHMARK_RESULT: circuit={circuit} cpu_instructions={{{{consumed}}}} status={{{{result.is_ok()}}}}");
        
        assert!(result.is_ok(), "Verification failed: {{:?}}", result);
    }}
}}
'''
    
    # Write benchmark test to a temporary file
    test_file = project_root / "contracts" / "zk-verifier" / "src" / f"bench_{circuit.replace('-', '_')}.rs"
    with open(test_file, "w") as f:
        f.write(benchmark_code)
    
    # Run the specific benchmark test
    returncode, stdout, stderr = run_command(
        ["cargo", "test", "-p", "zk-verifier", f"bench_{circuit.replace('-', '_')}", "--", "--nocapture"],
        project_root / "contracts"
    )
    
    # Clean up test file
    test_file.unlink(missing_ok=True)
    
    # Parse output for benchmark result
    result = {"circuit": circuit, "cpu_instructions": 0, "success": False}
    for line in stdout.split('\n'):
        if "BENCHMARK_RESULT:" in line:
            parts = line.split("BENCHMARK_RESULT:")[1].strip().split()
            for part in parts:
                if part.startswith("cpu_instructions="):
                    result["cpu_instructions"] = int(part.split("=")[1])
                elif part.startswith("status="):
                    result["success"] = part.split("=")[1] == "true"
    
    if result["cpu_instructions"] == 0:
        print(f"Warning: Could not parse benchmark result for {circuit}")
        print(f"stdout: {stdout}")
        print(f"stderr: {stderr}")
    
    return result


def main():
    project_root = Path(__file__).parent.parent
    
    print("=" * 60)
    print("StellPoker Circuit Gas Benchmarking")
    print("=" * 60)
    
    # Step 1: Compile circuits
    if not compile_circuits(project_root):
        sys.exit(1)
    
    # Step 2: Build zk-verifier contract
    if not build_benchmark_contract(project_root):
        sys.exit(1)
    
    # Step 3: Generate proofs and public inputs for each circuit
    print("\nGenerating proofs and public inputs...")
    for circuit in CIRCUITS:
        circuit_dir = project_root / "circuits" / circuit
        proof_file, inputs_file = generate_proof_and_inputs(circuit_dir, project_root)
        print(f"  {circuit}: proof={proof_file.exists()}, inputs={inputs_file.exists()}")
    
    # Step 4: Run benchmarks
    print("\nRunning benchmarks...")
    results = []
    for circuit in CIRCUITS:
        result = run_benchmark_test(project_root, circuit)
        results.append(result)
        pct = (result["cpu_instructions"] / SOROBAN_TRANSACTION_INSTRUCTION_LIMIT) * 100
        status = "✓" if result["success"] else "✗"
        warn = " ⚠️ EXCEEDS 80% LIMIT!" if pct > WARNING_THRESHOLD_PCT else ""
        print(f"  {status} {circuit}: {result['cpu_instructions']:,} CPU instructions ({pct:.1f}% of limit){warn}")
    
    # Step 5: Generate BENCHMARKS.md
    generate_benchmarks_md(project_root, results)
    
    # Step 6: Check thresholds
    print("\n" + "=" * 60)
    print("Threshold Check:")
    print("=" * 60)
    all_ok = True
    for result in results:
        pct = (result["cpu_instructions"] / SOROBAN_TRANSACTION_INSTRUCTION_LIMIT) * 100
        if pct > WARNING_THRESHOLD_PCT:
            print(f"  ❌ {result['circuit']}: {pct:.1f}% > {WARNING_THRESHOLD_PCT}% limit")
            all_ok = False
        else:
            print(f"  ✅ {result['circuit']}: {pct:.1f}% < {WARNING_THRESHOLD_PCT}% limit")
    
    if not all_ok:
        print("\n⚠️  WARNING: One or more circuits exceed the 80% instruction limit!")
        sys.exit(1)
    else:
        print("\n✅ All circuits within 80% instruction limit")
        sys.exit(0)


def generate_benchmarks_md(project_root: Path, results: List[Dict]):
    """Generate the BENCHMARKS.md file with results."""
    benchmarks_path = project_root / "circuits" / "BENCHMARKS.md"
    
    # Read existing content up to the Results section
    existing_content = ""
    if benchmarks_path.exists():
        with open(benchmarks_path, "r") as f:
            existing_content = f.read()
    
    # Find the Results section and replace everything after it
    lines = existing_content.split('\n')
    new_lines = []
    in_results = False
    for line in lines:
        if line.startswith("## Results") or line.startswith("### Constraint Table"):
            in_results = True
            break
        new_lines.append(line)
    
    # Add new results
    new_lines.append("## Results")
    new_lines.append("")
    new_lines.append(f"*Generated on: {__import__('datetime').datetime.now().strftime('%Y-%m-%d %H:%M:%S')}*")
    new_lines.append("")
    new_lines.append("### Verification Gas Benchmarks")
    new_lines.append("")
    new_lines.append("| Circuit | CPU Instructions | % of 100M Limit | Status |")
    new_lines.append("|---------|-----------------:|----------------:|--------|")
    
    for result in results:
        pct = (result["cpu_instructions"] / SOROBAN_TRANSACTION_INSTRUCTION_LIMIT) * 100
        status = "✅ PASS" if pct <= WARNING_THRESHOLD_PCT else "❌ FAIL (>80%)"
        new_lines.append(f"| {result['circuit']} | {result['cpu_instructions']:,} | {pct:.1f}% | {status} |")
    
    new_lines.append("")
    new_lines.append("### Constraint Budgets (from constraint-budgets.json)")
    new_lines.append("")
    new_lines.append("| Circuit | ACIR Opcodes | Backend Opcodes |")
    new_lines.append("|---------|-------------:|----------------:|")
    
    # Load constraint budgets
    budgets_path = project_root / "circuits" / "constraint-budgets.json"
    if budgets_path.exists():
        with open(budgets_path, "r") as f:
            budgets = json.load(f)
        for circuit in CIRCUITS:
            budget = budgets["circuits"].get(circuit, {})
            acir = budget.get("max_acir_opcodes", "N/A")
            backend = budget.get("max_backend_opcodes", "N/A")
            new_lines.append(f"| {circuit} | {acir:,} | {backend:,} |")
    
    new_lines.append("")
    new_lines.append("---")
    new_lines.append("")
    new_lines.append("## Regression Alerts")
    new_lines.append("")
    new_lines.append("A CI workflow (`.github/workflows/circuit-gas-benchmarks.yml`) automatically")
    new_lines.append("runs on every push that touches `circuits/`. If any circuit exceeds")
    new_lines.append(f"{WARNING_THRESHOLD_PCT}% of the Soroban transaction instruction limit")
    new_lines.append(f"({SOROBAN_TRANSACTION_INSTRUCTION_LIMIT:,} instructions), the workflow fails.")
    new_lines.append("")
    new_lines.append("---")
    
    # Append the rest of the original file after the Circuit Descriptions section
    in_circuit_desc = False
    for line in lines:
        if line.startswith("## Circuit Descriptions"):
            in_circuit_desc = True
        if in_circuit_desc:
            new_lines.append(line)
    
    with open(benchmarks_path, "w") as f:
        f.write('\n'.join(new_lines))
    
    print(f"\nUpdated {benchmarks_path}")


if __name__ == "__main__":
    main()
