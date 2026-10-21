import type { CrashFix, LauncherError } from './schema';

/** `?crash=<scenario>` in dev:mock: the game crashes after a few seconds with this cause. */
export type CrashScenario =
  | 'memory'
  | 'driver'
  | 'java'
  | 'missing'
  | 'clash'
  | 'version'
  | 'mixin'
  | 'damaged_mod'
  | 'files'
  | 'unknown';

export interface MockCrash {
  line: string;
  cause: LauncherError | null;
  fixes: CrashFix[];
}

const SODIUM = 'sodium-fabric-0.5.8+mc1.20.1.jar';
const FABRIC_API = 'fabric-api-0.92.2+1.20.1.jar';

function cause(
  category: LauncherError['category'],
  code: string,
  message: string,
  fix: string,
  line: string,
  params: Record<string, string> = {},
): LauncherError {
  return { category, code, message, fix, detail: line, params } as LauncherError;
}

const SCENARIOS: Record<CrashScenario, () => MockCrash> = {
  memory: () => {
    const line = 'Exception in thread "Render thread" java.lang.OutOfMemoryError: Java heap space';
    return {
      line,
      cause: cause('game', 'crash_out_of_memory', 'Minecraft ran out of memory', 'Give it more memory and press Play again', line),
      fixes: [{ kind: 'more_memory' }],
    };
  },
  driver: () => {
    const line = '# C  [nvoglv64.dll+0xb3c2a0]';
    return {
      line,
      cause: cause('game', 'crash_graphics_driver', 'Your graphics driver crashed the game', 'Update your graphics driver', line, { vendor: 'nvidia' }),
      fixes: [{ kind: 'update_drivers', vendor: 'nvidia' }],
    };
  },
  java: () => {
    const line =
      "- Mod 'Sodium' (sodium) 0.6.0 requires version 21 or later of 'OpenJDK 64-Bit Server VM' (java), but only the wrong version is present: 17!";
    return {
      line,
      cause: cause('java', 'crash_java_too_old', 'This game needs Java 21 or newer', 'Switch to a newer Java', line, { required: '21', running: '17' }),
      fixes: [{ kind: 'java_settings' }],
    };
  },
  missing: () => {
    const line = "- Mod 'Iris' (iris) 1.7.0+mc1.20.1 requires any version of fabric-api, which is missing!";
    return {
      line,
      cause: cause('loader', 'crash_mod_missing_dependency', "Iris needs fabric-api, which isn't installed", 'Add it', line, {
        mod: 'Iris',
        missing: 'fabric-api',
      }),
      fixes: [{ kind: 'find_mod', project: 'fabric-api' }],
    };
  },
  clash: () => {
    const line =
      "- Mod 'Sodium' (sodium) 0.5.8+mc1.20.1 is incompatible with any version of mod 'Fabric API' (fabric-api), but only the wrong version is present: 0.92.2!";
    return {
      line,
      cause: cause('loader', 'crash_mods_clash', "Sodium and Fabric API don't work together", 'Turn one of them off', line, {
        mod: 'Sodium',
        other: 'Fabric API',
      }),
      fixes: [
        { kind: 'disable_mod', file: FABRIC_API, name: 'Fabric API' },
        { kind: 'disable_mod', file: SODIUM, name: 'Sodium' },
      ],
    };
  },
  version: () => {
    const line =
      "- Mod 'Sodium' (sodium) 0.6.0 requires version 1.21 of 'Minecraft' (minecraft), but only the wrong version is present: 1.20.1!";
    return {
      line,
      cause: cause('loader', 'crash_mod_wrong_version', 'Sodium is made for a different version of Minecraft', 'Turn it off', line, {
        mod: 'Sodium',
      }),
      fixes: [{ kind: 'disable_mod', file: SODIUM, name: 'Sodium' }],
    };
  },
  mixin: () => {
    const line =
      'Mixin apply for mod sodium failed sodium.mixins.json:features.render.world.WorldRendererMixin from mod sodium -> net.minecraft.class_761';
    return {
      line,
      cause: cause('loader', 'crash_mod_failed', 'Sodium broke while the game was loading', 'Turn it off', line, { mod: 'Sodium' }),
      fixes: [{ kind: 'disable_mod', file: SODIUM, name: 'Sodium' }],
    };
  },
  damaged_mod: () => {
    const line = `Error analyzing [~/PerlandLauncher/instances/asalith-fields/minecraft/mods/${SODIUM}]: java.util.zip.ZipException: zip END header not found`;
    return {
      line,
      cause: cause('integrity', 'crash_mod_file_damaged', `The file for ${SODIUM} is damaged`, 'Turn it off and download it again', line, {
        mod: SODIUM,
      }),
      fixes: [{ kind: 'disable_mod', file: SODIUM, name: SODIUM }],
    };
  },
  files: () => {
    const line = 'Error: Could not find or load main class net.minecraft.client.main.Main';
    return {
      line,
      cause: cause('integrity', 'crash_game_files_damaged', 'Some game files are damaged', 'Repair them and play again', line),
      fixes: [{ kind: 'repair' }],
    };
  },
  unknown: () => ({ line: 'Exception in thread "main" java.lang.IllegalStateException: something unusual', cause: null, fixes: [] }),
};

export function crashScenario(raw: string | null): CrashScenario | undefined {
  if (raw === null) return undefined;
  return raw in SCENARIOS ? (raw as CrashScenario) : 'memory';
}

export function mockCrash(scenario: CrashScenario): MockCrash {
  return SCENARIOS[scenario]();
}
