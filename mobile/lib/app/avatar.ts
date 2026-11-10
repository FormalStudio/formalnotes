// Picking the profile picture: the system photo picker, then the photo's centre square as a 512
// pixel JPEG the sync controller seals and uploads.

import { manipulateAsync, SaveFormat } from 'expo-image-manipulator';
import { launchImageLibraryAsync } from 'expo-image-picker';
import { ActionSheetIOS, Alert } from 'react-native';

import { haptic } from '../haptics';
import { accountError, sync } from './sync';

const SIDE = 512;

async function run(change: () => Promise<void>, setBusy: (busy: boolean) => void) {
  setBusy(true);
  try {
    await change();
    haptic('success');
  } catch (e) {
    haptic('warning');
    Alert.alert('The picture was not saved', accountError(e));
  } finally {
    setBusy(false);
  }
}

async function choose(setBusy: (busy: boolean) => void) {
  const picked = await launchImageLibraryAsync({ mediaTypes: ['images'], quality: 1 });
  const asset = picked.assets?.[0];
  if (picked.canceled || !asset) return;
  await run(async () => {
    const side = Math.min(asset.width, asset.height);
    const crop = { originX: (asset.width - side) / 2, originY: (asset.height - side) / 2, width: side, height: side };
    const square = await manipulateAsync(asset.uri, [{ crop }, { resize: { width: SIDE, height: SIDE } }], {
      compress: 0.8,
      format: SaveFormat.JPEG,
      base64: true,
    });
    if (!square.base64) throw new Error('The photo could not be read.');
    await sync.setAvatar(square.base64);
  }, setBusy);
}

/** The menu behind the picture: choose a photo, or remove the one there is. */
export function changeAvatar(hasPhoto: boolean, setBusy: (busy: boolean) => void): void {
  const remove = () => void run(() => sync.removeAvatar(), setBusy);
  if (process.env.EXPO_OS !== 'ios') {
    Alert.alert('Profile picture', undefined, [
      { text: 'Choose photo', onPress: () => void choose(setBusy) },
      ...(hasPhoto ? [{ text: 'Remove photo', style: 'destructive' as const, onPress: remove }] : []),
      { text: 'Cancel', style: 'cancel' },
    ]);
    return;
  }
  const options = hasPhoto ? ['Choose Photo', 'Remove Photo', 'Cancel'] : ['Choose Photo', 'Cancel'];
  ActionSheetIOS.showActionSheetWithOptions(
    { options, cancelButtonIndex: options.length - 1, destructiveButtonIndex: hasPhoto ? 1 : undefined },
    (index) => {
      if (index === 0) void choose(setBusy);
      else if (hasPhoto && index === 1) remove();
    },
  );
}
