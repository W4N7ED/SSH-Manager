/**
 * Génération de mots de passe.
 *
 * L'aléa vient de `crypto.getRandomValues` : `Math.random` n'est pas
 * cryptographiquement sûr et ne doit jamais servir à produire un secret.
 */

export interface PasswordOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  /** Retire les caractères que l'on confond à la lecture (l/1/I, O/0…). */
  excludeAmbiguous: boolean;
}

export const PASSWORD_LENGTH_MIN = 8;
export const PASSWORD_LENGTH_MAX = 64;

export const DEFAULT_PASSWORD_OPTIONS: PasswordOptions = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  excludeAmbiguous: false,
};

const CHARACTER_SETS = {
  lowercase: 'abcdefghijklmnopqrstuvwxyz',
  uppercase: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  digits:    '0123456789',
  symbols:   '!@#$%^&*()-_=+[]{}:;,.?/',
} as const;

const AMBIGUOUS_CHARACTERS = 'Il1O0o';

type CharacterSetName = keyof typeof CHARACTER_SETS;

/** Uniform integer in [0, max) — rejection sampling avoids the modulo bias. */
function randomIndex(max: number): number {
  const limit = Math.floor(0xffffffff / max) * max;
  const buffer = new Uint32Array(1);
  let value: number;
  do {
    crypto.getRandomValues(buffer);
    value = buffer[0];
  } while (value >= limit);
  return value % max;
}

function shuffle(characters: string[]): string[] {
  for (let i = characters.length - 1; i > 0; i--) {
    const j = randomIndex(i + 1);
    [characters[i], characters[j]] = [characters[j], characters[i]];
  }
  return characters;
}

/** The character sets enabled by the options, ambiguity filter applied. */
export function resolveCharacterSets(options: PasswordOptions): string[] {
  const names = (Object.keys(CHARACTER_SETS) as CharacterSetName[]).filter(name => options[name]);
  return names
    .map(name => options.excludeAmbiguous
      ? [...CHARACTER_SETS[name]].filter(c => !AMBIGUOUS_CHARACTERS.includes(c)).join('')
      : CHARACTER_SETS[name])
    .filter(set => set.length > 0);
}

/**
 * Generates a password honouring every enabled set: each contributes at least
 * one character whenever the requested length allows it.
 * Returns an empty string when no character set is enabled.
 */
export function generatePassword(options: PasswordOptions): string {
  const sets = resolveCharacterSets(options);
  if (sets.length === 0) return '';

  const pool = sets.join('');
  const length = Math.max(PASSWORD_LENGTH_MIN, Math.min(PASSWORD_LENGTH_MAX, options.length));

  const characters = sets.slice(0, length).map(set => set[randomIndex(set.length)]);
  while (characters.length < length) {
    characters.push(pool[randomIndex(pool.length)]);
  }
  return shuffle(characters).join('');
}

/** Shannon entropy of the generation process, in bits. */
export function passwordEntropyBits(options: PasswordOptions): number {
  const pool = resolveCharacterSets(options).join('');
  if (pool.length === 0) return 0;
  return Math.round(options.length * Math.log2(pool.length));
}

export function describeStrength(entropyBits: number): { label: string; level: 'weak' | 'fair' | 'strong' | 'excellent' } {
  if (entropyBits < 50)  return { label: 'faible', level: 'weak' };
  if (entropyBits < 70)  return { label: 'correcte', level: 'fair' };
  if (entropyBits < 100) return { label: 'solide', level: 'strong' };
  return { label: 'excellente', level: 'excellent' };
}
