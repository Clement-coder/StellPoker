#!/usr/bin/env python3
"""
Generates an ER diagram (Mermaid format) and detailed documentation
from the coordinator PostgreSQL migration files (*.up.sql).

Usage:
    python3 scripts/db/generate_er_diagram.py
Output:
    docs/database-schema.md
"""

import glob
import re
import os

MIGRATIONS_DIR = os.path.join(os.path.dirname(__file__), "../../services/coordinator/migrations")
OUTPUT_PATH = os.path.join(os.path.dirname(__file__), "../../docs/database-schema.md")

def parse_sql_files(migrations_dir):
    sql_files = sorted(glob.glob(os.path.join(migrations_dir, "*.up.sql")))
    
    tables = {}
    relationships = []

    create_table_regex = re.compile(r"CREATE\ TABLE\ (?:IF\ NOT\ EXISTS\ )?([a-z0_9_]+)\s*\((.*?)\)\s*;", re.DOTALL | re.IGNORECASE)
    
    for file_path in sql_files:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()

        # Remove single line comments
        cleaned_lines = []
        for line in content.splitlines():
            if line.strip().startswith("--"):
                continue
            cleaned_lines.append(line)
        cleaned_content = "\n".join(cleaned_lines)

        for match in create_table_regex.finditer(cleaned_content):
            table_name = match.group(1).lower()
            body = match.group(2)
            
            columns = []
            
            # Split body by lines or commas, handling nested parens/constraints
            lines = [line.strip() for line in body.splitlines() if line.strip()]
            for line in lines:
                # Skip table-level constraints or rules for line parsing if they don't start with column name
                line_clean = line.rstrip(",")
                if not line_clean or line_clean.upper().startswith(("CONSTRAINT", "UNIQUE", "PRIMARY KEY", "FOREIGN KEY", "CHECK")):
                    # Check for table level FOREIGN KEY
                    fk_match = re.search(r"FOREIGN\ KEY\s*\(([a-z0-9_]+)\)\s*REFERENCES\s*([a-z0-9_]+)\s*\(([a-z0-9_]+)\)", line_clean, re.IGNORECASE)
                    if fk_match:
                        from_col, to_table, to_col = fk_match.groups()
                        relationships.append((table_name, to_table.lower(), from_col.lower(), to_col.lower()))
                    continue
                
                parts = line_clean.split(maxsplit=2)
                if len(parts) < 2:
                    continue
                
                col_name = parts[0].rstrip(",").lower()
                col_type = parts[1].rstrip(",").upper()
                rest = parts[2] if len(parts) > 2 else ""
                
                is_pk = "PRIMARY KEY" in line_clean.upper()
                is_fk = "REFERENCES" in line_clean.upper()
                is_unique = "UNIQUE" in line_clean.upper()
                is_nullable = "NOT NULL" not in line_clean.upper()
                
                fk_ref = None
                if is_fk:
                    ref_match = re.search(r"REFERENCES\s+([a-z0-9_]+)\s*(?:\(([a-z0-9_]+)\))?", line_clean, re.IGNORECASE)
                    if ref_match:
                        ref_table = ref_match.group(1).lower()
                        ref_col = ref_match.group(2).lower() if ref_match.group(2) else "id"
                        fk_ref = (ref_table, ref_col)
                        relationships.append((table_name, ref_table, col_name, ref_col))
                
                columns.append({
                    "name": col_name,
                    "type": col_type,
                    "is_pk": is_pk,
                    "is_fk": is_fk,
                    "is_unique": is_unique,
                    "is_nullable": is_nullable,
                    "fk_ref": fk_ref
                })
            
            tables[table_name] = columns

    return tables, relationships

def generate_markdown(tables, relationships):
    md = []
    md.append("# 🗄️ Coordinator Database Schema & ER Diagram")
    md.append("")
    md.append("> **Auto-generated reference.** Updates automatically when database migrations are modified.")
    md.append("")
    md.append("## Entity-Relationship Diagram")
    md.append("")
    md.append("```mermaid")
    md.append("erDiagram")
    
    # Render tables in Mermaid format
    for table_name, columns in tables.items():
        md.append(f"    {table_name} {{")
        for col in columns:
            col_type = col['type'].replace(" ", "_")
            keys = []
            if col['is_pk']:
                keys.append("PK")
            if col['is_fk']:
                keys.append("FK")
            if col['is_unique'] and not col['is_pk']:
                keys.append("UK")
            key_str = ",".join(keys)
            
            if key_str:
                md.append(f"        {col_type} {col['name']} {key_str}")
            else:
                md.append(f"        {col_type} {col['name']}")
        md.append("    }")
    
    # Deduplicate relationships
    seen_rel = set()
    for from_table, to_table, from_col, to_col in relationships:
        rel_key = (from_table, to_table, from_col)
        if rel_key not in seen_rel:
            seen_rel.add(rel_key)
            md.append(f'    {to_table} ||--o{{ {from_table} : "{from_col}"')
            
    md.append("```")
    md.append("")
    md.append("## Tables Detail")
    md.append("")

    for table_name, columns in tables.items():
        md.append(f"### `{table_name}`")
        md.append("")
        md.append("| Column | Type | Constraints | Reference |")
        md.append("| --- | --- | --- | --- |")
        for col in columns:
            constraints = []
            if col['is_pk']:
                constraints.append("PRIMARY KEY")
            if not col['is_nullable'] and not col['is_pk']:
                constraints.append("NOT NULL")
            if col['is_unique']:
                constraints.append("UNIQUE")
            
            constraint_str = ", ".join(constraints) if constraints else "Nullable"
            ref_str = f"`{col['fk_ref'][0]}.${col['fk_ref'][1]}`" if col['fk_ref'] else "-"
            
            md.append(f"| `{col['name']}` | `{col['type']}` | {constraint_str} | {ref_str} |")
        md.append("")

    return "\n".join(md)

def main():
    abs_migrations = os.path.abspath(MIGRATIONS_DIR)
    abs_output = os.path.abspath(OUTPUT_PATH)
    
    tables, relationships = parse_sql_files(abs_migrations)
    markdown_content = generate_markdown(tables, relationships)
    
    os.makedirs(os.path.dirname(abs_output), exist_ok=True)
    with open(abs_output, "w", encoding="utf-8") as f:
        f.write(markdown_content)
        
    print(f"Generated ER diagram and schema docs at: {abs_output}")

if __name__ == "__main__":
    main()
