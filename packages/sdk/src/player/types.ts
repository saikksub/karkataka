import type { Clip, Project, Track } from "../project.ts";

/** A clip with what the player needs to load its media, added by whoever hosts the player. */
export type PlayerClip = Clip & {
  /** URL of the clip's image or video file. */
  url?: string;
  /** Absolute path of a file with sound, for karka's audio mixer. */
  path?: string;
  /** Video frames as `<url>000001.jpg`...: the file for the clip's frame `first + i` is number `i + 1`. Without it, a <video> is seeked instead (Studio preview). */
  frames?: { url: string; first: number; count: number };
  /** The source has sound. */
  audio?: boolean;
};

export type PlayerProject = Omit<Project, "tracks" | "duration"> & {
  duration: number;
  /** The project's folder, ending in "/": where a component's relative <Audio src> is found. */
  base?: string;
  tracks: (Omit<Track, "clips"> & { clips: PlayerClip[] })[] };

/** A component clip's module: its default export, and an optional props schema for the Studio. */
export type ComponentModule = { default?: (props: Record<string, unknown>) => unknown; schema?: Record<string, unknown> };

declare global {
  interface Window {
    /** The Studio sends the project here as it changes. */
    __karkaSetProject?: (project: PlayerProject) => void;
    /** Component modules, for the Studio's props panel. */
    __karkaComponents?: Record<string, ComponentModule>;
  }
}
