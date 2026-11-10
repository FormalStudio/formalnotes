import type { ReactNode, Ref } from 'react';
import { ActivityIndicator, Pressable, Text, TextInput, View, type TextInputInstance, type TextInputProps } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { colors } from '@/lib/ui/colors';

/** A sheet's body: a scrolling column that keeps the focused field above the keyboard. */
export function Form({ children }: { children: ReactNode }) {
  return (
    <KeyboardAwareScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      bottomOffset={24}
      contentContainerStyle={{ padding: 20, gap: 16 }}
    >
      {children}
    </KeyboardAwareScrollView>
  );
}

export function Field({ label, ref, ...input }: TextInputProps & { label: string; ref?: Ref<TextInputInstance> }) {
  return (
    <View style={{ gap: 6 }}>
      <Text style={{ fontSize: 13, fontWeight: '600', color: colors.secondary }}>{label}</Text>
      <TextInput
        ref={ref}
        accessibilityLabel={label}
        placeholderTextColor={colors.tertiary}
        {...input}
        style={{
          minHeight: 48,
          paddingHorizontal: 14,
          borderRadius: 12,
          borderCurve: 'continuous',
          backgroundColor: colors.fill,
          fontSize: 17,
          color: colors.text,
        }}
      />
    </View>
  );
}

export function FormButton({
  title,
  onPress,
  busy,
  disabled,
  destructive,
}: {
  title: string;
  onPress: () => void;
  busy?: boolean;
  disabled?: boolean;
  destructive?: boolean;
}) {
  const off = disabled || busy;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ disabled: off, busy }}
      disabled={off}
      onPress={onPress}
      style={({ pressed }) => ({
        minHeight: 50,
        borderRadius: 25,
        borderCurve: 'continuous',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: destructive ? colors.danger : colors.accent,
        opacity: off ? 0.4 : pressed ? 0.75 : 1,
        transform: [{ scale: pressed ? 0.98 : 1 }],
      })}
    >
      {busy ? <ActivityIndicator color="white" /> : <Text style={{ fontSize: 17, fontWeight: '600', color: 'white' }}>{title}</Text>}
    </Pressable>
  );
}

export function FormError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <Text selectable accessibilityLiveRegion="polite" style={{ fontSize: 15, color: colors.danger }}>
      {message}
    </Text>
  );
}

export function FormNote({ children }: { children: ReactNode }) {
  return <Text style={{ fontSize: 13, lineHeight: 18, color: colors.secondary }}>{children}</Text>;
}

export function FormLink({ title, onPress }: { title: string; onPress: () => void }) {
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      hitSlop={8}
      style={({ pressed }) => ({ alignSelf: 'center', paddingVertical: 4, opacity: pressed ? 0.5 : 1 })}
    >
      <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>{title}</Text>
    </Pressable>
  );
}
