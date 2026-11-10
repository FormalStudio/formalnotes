// The picture an account has before it picks a photo: the email's first letter on two
// neighbouring hues chosen from the email, so it stays the same on every launch.

function fnv1a(text: string): bigint {
  let hash = 0xcbf29ce484222325n;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash;
}

/** Both in degrees, the second 30 to 60 further round the wheel. */
export function avatarHues(seed: string): [number, number] {
  const hash = fnv1a(seed);
  const first = Number(hash % 360n);
  return [first, (first + 30 + Number((hash >> 16n) % 31n)) % 360];
}

export function avatarGradient(seed: string): string {
  const [first, second] = avatarHues(seed);
  return `linear-gradient(135deg, hsl(${first} 92% 74%) 0%, hsl(${second} 62% 46%) 100%)`;
}

export function avatarLetter(email: string): string {
  const first = [...email.trim()][0];
  return first ? first.toUpperCase() : '';
}
