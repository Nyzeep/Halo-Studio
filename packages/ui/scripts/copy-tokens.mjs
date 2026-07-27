import { cpSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const currentDirectory = dirname(fileURLToPath(import.meta.url));
const packageDirectory = resolve(currentDirectory, "..");
const outputDirectory = resolve(packageDirectory, "dist");

mkdirSync(outputDirectory, { recursive: true });
cpSync(resolve(packageDirectory, "src", "tokens.css"), resolve(outputDirectory, "tokens.css"));
