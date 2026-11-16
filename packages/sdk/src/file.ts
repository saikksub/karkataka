// Project files on disk.

import { existsSync } from "node:fs";
import { readFile, rename, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import type { Project } from "./project.ts";

export async function loadProject(path: string): Promise<Project> {
  let text: string;
  try {
    text = await readFile(path, "utf8");
  } catch (e) {
    throw new Error(`cannot read ${path}: ${(e as Error).message}`);
  }
  try {
    return JSON.parse(text);
  } catch (e) {
    throw new Error(`${path} is not valid JSON: ${(e as Error).message}`);
  }
}

/** Write the project as readable JSON. Written to a temporary file first, so a crash never leaves half a project. */
export async function saveProject(path: string, project: Project): Promise<void> {
  const tmp = `${path}.${process.pid}.tmp`;
  await writeFile(tmp, JSON.stringify(project, null, 2) + "\n");
  await rename(tmp, path);
}

/** A path written in the project (`src`, `component`), as an absolute path. */
export function resolveIn(projectPath: string, path: string): string {
  return isAbsolute(path) ? path : resolve(dirname(projectPath), path);
}

/** For `validate`: whether a path written in the project exists. */
export const filesNextTo = (projectPath: string) => (path: string) => existsSync(resolveIn(projectPath, path));
