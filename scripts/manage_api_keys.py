#!/usr/bin/env python3
"""
API Key management CLI for Stellar Poker coordinator.

Usage:
    python3 scripts/manage_api_keys.py create --node-id node0 --description "Node 0 MPC key"
    python3 scripts/manage_api_keys.py list --node-id node0
    python3 scripts/manage_api_keys.py revoke --key-id key_abc123 --reason "Compromised"
    python3 scripts/manage_api_keys.py rotate --node-id node0
"""

import argparse
import json
import os
import requests
import sys
from datetime import datetime, timedelta

COORDINATOR_URL = os.environ.get("COORDINATOR_URL", "http://localhost:8080")
ADMIN_SECRET = os.environ.get("ADMIN_SECRET")

def make_admin_request(method, path, data=None):
    """Make an authenticated admin request."""
    if not ADMIN_SECRET:
        print("Error: ADMIN_SECRET environment variable not set")
        sys.exit(1)
    
    headers = {
        "Content-Type": "application/json",
        "x-admin-address": "ADMIN",  # For dev mode
        "x-admin-signature": "dev",
        "x-admin-nonce": "1",
        "x-admin-timestamp": str(int(datetime.now().timestamp()))
    }
    
    url = f"{COORDINATOR_URL}{path}"
    
    if method == "GET":
        response = requests.get(url, headers=headers)
    elif method == "POST":
        response = requests.post(url, headers=headers, json=data)
    elif method == "DELETE":
        response = requests.delete(url, headers=headers)
    else:
        raise ValueError(f"Unsupported method: {method}")
    
    return response

def create_api_key(args):
    """Create a new API key."""
    data = {
        "node_id": args.node_id,
        "description": args.description,
    }
    
    if args.expires_days:
        expires_at = datetime.now() + timedelta(days=args.expires_days)
        data["expires_at"] = expires_at.isoformat() + "Z"
    
    response = make_admin_request("POST", "/api/admin/api-keys", data)
    
    if response.status_code == 200:
        result = response.json()
        print("✅ API key created successfully!")
        print(f"Key ID: {result['key_info']['key_id']}")
        print(f"API Key: {result['api_key']}")
        print(f"Node ID: {result['key_info']['node_id']}")
        print("\n⚠️  Save this API key securely - it won't be shown again!")
    else:
        print(f"❌ Failed to create API key: {response.status_code}")
        print(response.text)

def list_api_keys(args):
    """List API keys for a node."""
    response = make_admin_request("GET", f"/api/admin/api-keys/{args.node_id}")
    
    if response.status_code == 200:
        keys = response.json()
        if not keys:
            print(f"No API keys found for node: {args.node_id}")
            return
        
        print(f"API Keys for node: {args.node_id}")
        print("=" * 80)
        for key in keys:
            status = "🟢 Active" if key["is_active"] else "🔴 Inactive"
            expires = key.get("expires_at", "Never")
            if expires != "Never":
                expires = datetime.fromisoformat(expires.replace("Z", "")).strftime("%Y-%m-%d %H:%M")
            
            print(f"Key ID: {key['key_id']}")
            print(f"Status: {status}")
            print(f"Description: {key.get('description', 'N/A')}")
            print(f"Created: {datetime.fromisoformat(key['created_at'].replace('Z', '')).strftime('%Y-%m-%d %H:%M')}")
            print(f"Expires: {expires}")
            print(f"Last Used: {key.get('last_used_at', 'Never')}")
            if key.get("revoked_at"):
                print(f"Revoked: {key['revoked_at']} ({key.get('revoked_reason', 'No reason')})")
            print("-" * 80)
    else:
        print(f"❌ Failed to list API keys: {response.status_code}")
        print(response.text)

def revoke_api_key(args):
    """Revoke an API key."""
    data = {"reason": args.reason} if args.reason else {}
    
    response = make_admin_request("POST", f"/api/admin/api-keys/{args.key_id}/revoke", data)
    
    if response.status_code == 200:
        print(f"✅ API key {args.key_id} revoked successfully")
    elif response.status_code == 404:
        print(f"❌ API key {args.key_id} not found")
    else:
        print(f"❌ Failed to revoke API key: {response.status_code}")
        print(response.text)

def rotate_api_key(args):
    """Rotate an API key (create new, list old for manual revocation)."""
    # First, list existing keys
    response = make_admin_request("GET", f"/api/admin/api-keys/{args.node_id}")
    if response.status_code != 200:
        print(f"❌ Failed to check existing keys: {response.status_code}")
        return
    
    existing_keys = response.json()
    active_keys = [k for k in existing_keys if k["is_active"]]
    
    # Create new key
    create_data = {
        "node_id": args.node_id,
        "description": f"Rotated key - {datetime.now().strftime('%Y-%m-%d')}",
    }
    
    response = make_admin_request("POST", "/api/admin/api-keys", create_data)
    
    if response.status_code == 200:
        result = response.json()
        print("✅ New API key created successfully!")
        print(f"New Key ID: {result['key_info']['key_id']}")
        print(f"New API Key: {result['api_key']}")
        print("\n⚠️  Save this API key securely - it won't be shown again!")
        
        if active_keys:
            print("\n📋 Old active keys to revoke after updating MPC nodes:")
            for key in active_keys:
                print(f"  - {key['key_id']} (created: {key['created_at']})")
            print(f"\nUse: python3 {sys.argv[0]} revoke --key-id <KEY_ID> --reason 'Key rotation'")
    else:
        print(f"❌ Failed to create new API key: {response.status_code}")
        print(response.text)

def main():
    parser = argparse.ArgumentParser(description="Manage API keys for MPC nodes")
    subparsers = parser.add_subparsers(dest="command", help="Commands")
    
    # Create command
    create_parser = subparsers.add_parser("create", help="Create a new API key")
    create_parser.add_argument("--node-id", required=True, help="MPC node identifier")
    create_parser.add_argument("--description", help="Key description")
    create_parser.add_argument("--expires-days", type=int, help="Key expiration in days")
    
    # List command
    list_parser = subparsers.add_parser("list", help="List API keys for a node")
    list_parser.add_argument("--node-id", required=True, help="MPC node identifier")
    
    # Revoke command
    revoke_parser = subparsers.add_parser("revoke", help="Revoke an API key")
    revoke_parser.add_argument("--key-id", required=True, help="API key ID to revoke")
    revoke_parser.add_argument("--reason", help="Revocation reason")
    
    # Rotate command
    rotate_parser = subparsers.add_parser("rotate", help="Rotate API keys for a node")
    rotate_parser.add_argument("--node-id", required=True, help="MPC node identifier")
    
    args = parser.parse_args()
    
    if not args.command:
        parser.print_help()
        return
    
    if args.command == "create":
        create_api_key(args)
    elif args.command == "list":
        list_api_keys(args)
    elif args.command == "revoke":
        revoke_api_key(args)
    elif args.command == "rotate":
        rotate_api_key(args)

if __name__ == "__main__":
    main()
