import { Activity, Headphones, Sparkles } from 'lucide-react';

export type PresetId = 'misa' | 'loudness' | 'fullEq';

export const DEFAULT_PRESET: PresetId = 'misa';

export const PRESET_TO_PROFILE: Partial<Record<PresetId, number>> = {
  fullEq: 4,
  loudness: 3,
  misa: 2,
};

export const PROFILE_TO_PRESET: Record<number, PresetId> = {
  2: 'misa',
  3: 'loudness',
  4: 'fullEq',
};

export const ROUTING_PRESETS = [
  {
    id: 'misa' as const,
    name: 'MISA',
    desc: 'Preamp -7.5 dB, MJUCjr, GraphicEQ brillante',
    icon: Sparkles,
    profileId: 2,
  },
  {
    id: 'loudness' as const,
    name: 'PURE',
    desc: 'MJUCjr 0.475, HP 70 Hz, GraphicEQ correction',
    icon: Activity,
    profileId: 3,
  },
  {
    id: 'fullEq' as const,
    name: 'VANY',
    desc: 'GraphicEQ completo + LoudnessCorrection',
    icon: Headphones,
    profileId: 4,
  },
];
