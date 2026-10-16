// Generates src/types/ir.ts from engine/schema/ir.schema.json.
//
//   pnpm --filter desktop gen:types     rewrite the checked-in file
//   pnpm --filter desktop check:types   fail if the checked-in file is stale
//
// `lint` runs the check, so CI fails when the schema changes without the generated types being
// regenerated.

import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { compile } from "json-schema-to-typescript";

const here = dirname(fileURLToPath(import.meta.url));

export const schemaPath = resolve(here, "../../../engine/schema/ir.schema.json");
export const outputPath = resolve(here, "../src/types/ir.ts");

const banner = `/**
 * Generated from engine/schema/ir.schema.json by scripts/gen-types.mjs. Do not edit by hand.
 * Run \`pnpm --filter desktop gen:types\` after changing the schema.
 */
`;

export async function generateIrTypes() {
  const schema = JSON.parse(await readFile(schemaPath, "utf8"));
  // The per-type "required" rules (table needs `table`, and so on) are validation only. Left in,
  // they make the generator emit an intersection and rename Block to Block1.
  delete schema.$defs.Block.allOf;
  const source = await compile(schema, "Document", {
    cwd: dirname(schemaPath),
    bannerComment: banner,
    additionalProperties: false,
    unreachableDefinitions: true,
    style: { printWidth: 100 },
  });
  return source.replace(/\r\n/g, "\n");
}

async function main() {
  const check = process.argv.includes("--check");
  const generated = await generateIrTypes();
  if (check) {
    const current = (await readFile(outputPath, "utf8")).replace(/\r\n/g, "\n");
    if (current !== generated) {
      console.error(
        "src/types/ir.ts is out of date. Run `pnpm --filter desktop gen:types` and commit the result.",
      );
      process.exit(1);
    }
    return;
  }
  await writeFile(outputPath, generated, "utf8");
  console.log(`wrote ${outputPath}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
