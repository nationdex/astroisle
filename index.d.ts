export interface AstroisleOptions {
  dist?: string;
  budgetInitial?: number;
  budgetLazy?: number;
  budgetCss?: number;
  nstdBin?: string;
  level?: number;
  noSw?: boolean;
  bin?: string;
}

export function findAstroisleBin(): string;
export function runAstroisle(options?: AstroisleOptions): void;

export default function astroisle(options?: AstroisleOptions): {
  name: string;
  hooks: {
    "astro:build:done": (options: { dir: URL | string }) => Promise<void>;
  };
};
