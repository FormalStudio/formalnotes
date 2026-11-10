import { expect, test } from 'bun:test';

import { avatarHues, avatarLetter } from './avatar';

test('the hues come from the email and stay 30 to 60 degrees apart', () => {
  for (const email of ['ada@example.com', 'grace@example.com', 'a@b.co']) {
    const [first, second] = avatarHues(email);
    expect(avatarHues(email)).toEqual([first, second]);
    expect(first).toBeGreaterThanOrEqual(0);
    expect(first).toBeLessThan(360);
    const apart = (second - first + 360) % 360;
    expect(apart).toBeGreaterThanOrEqual(30);
    expect(apart).toBeLessThanOrEqual(60);
  }
  expect(avatarHues('ada@example.com')).not.toEqual(avatarHues('grace@example.com'));
});

test('the letter is the first character, upper case', () => {
  expect(avatarLetter(' ada@example.com')).toBe('A');
  expect(avatarLetter('')).toBe('');
});
