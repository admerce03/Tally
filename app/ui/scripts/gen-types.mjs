// Generates src/lib/generated/{command,output}.ts from the committed core JSON schemas
// (written by `cargo run -p tally-core --features schema --example export_schema`).
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { compile } from "json-schema-to-typescript";

const dir = fileURLToPath(new URL("../src/lib/generated/", import.meta.url));
const banner =
  "/* eslint-disable */\n/**\n * GENERATED from core's JSON schema by `npm run gen:types`. Do not edit.\n */\n";

for (const name of ["command", "output"]) {
  const schema = JSON.parse(await readFile(`${dir}${name}.schema.json`, "utf8"));
  const ts = await compile(schema, name, {
    bannerComment: banner,
    additionalProperties: false,
    unreachableDefinitions: true,
    format: false,
  });
  await writeFile(`${dir}${name}.ts`, ts);
  console.log(`wrote ${name}.ts`);
}
