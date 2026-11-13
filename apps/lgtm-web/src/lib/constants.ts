import type { ColorOption } from 'lgtm-ui'
import {
  BookOpen,
  Box,
  Code,
  Compass,
  Cpu,
  Database,
  Globe,
  Layers,
  Package,
  Rocket,
  Server,
  Shield,
  Sparkles,
  Terminal,
  Wrench,
  Zap,
} from 'lucide-svelte'

/** Selectable project icons for `ProjectIconSelector` and `ProjectsPane`. */
export const PROJECT_ICONS: { id: string; label: string; component: typeof Box }[] = [
  { id: 'box', label: 'Box', component: Box },
  { id: 'package', label: 'Package', component: Package },
  { id: 'layers', label: 'Layers', component: Layers },
  { id: 'code', label: 'Code', component: Code },
  { id: 'terminal', label: 'Terminal', component: Terminal },
  { id: 'cpu', label: 'CPU', component: Cpu },
  { id: 'database', label: 'Database', component: Database },
  { id: 'globe', label: 'Globe', component: Globe },
  { id: 'rocket', label: 'Rocket', component: Rocket },
  { id: 'server', label: 'Server', component: Server },
  { id: 'shield', label: 'Shield', component: Shield },
  { id: 'sparkles', label: 'Sparkles', component: Sparkles },
  { id: 'wrench', label: 'Wrench', component: Wrench },
  { id: 'zap', label: 'Zap', component: Zap },
  { id: 'book', label: 'Book', component: BookOpen },
  { id: 'compass', label: 'Compass', component: Compass },
]

/** Selectable project color swatches for `ProjectEditDialog` and `ProjectsPane`. */
export const PROJECT_COLORS: ColorOption[] = [
  { value: 'default', label: 'Default', color: null },
  { value: 'blue', label: 'Blue', color: '#3b82f6' },
  { value: 'violet', label: 'Violet', color: '#8b5cf6' },
  { value: 'emerald', label: 'Emerald', color: '#10b981' },
  { value: 'amber', label: 'Amber', color: '#f59e0b' },
  { value: 'rose', label: 'Rose', color: '#f43f5e' },
  { value: 'cyan', label: 'Cyan', color: '#06b6d4' },
  { value: 'orange', label: 'Orange', color: '#f97316' },
  { value: 'pink', label: 'Pink', color: '#ec4899' },
]

/** Stormlight Archive-themed adjectives for default `worktree/<adjective>-<noun>-<digits>` branch names. */
export const WORKTREE_ADJECTIVES = [
  'radiant',
  'stormblessed',
  'windrunner',
  'lightweaver',
  'bondsmith',
  'skybreaker',
  'truthwatcher',
  'willshaper',
  'elsecaller',
  'stoneward',
  'edgedancer',
  'dustbringer',
] as const

/** Stormlight Archive-themed nouns for default `worktree/<adjective>-<noun>-<digits>` branch names. */
export const WORKTREE_NOUNS = [
  'spren',
  'shard',
  'highstorm',
  'urithiru',
  'shadesmar',
  'roshar',
  'kholinar',
  'bridge',
  'glyph',
  'oathgate',
  'sylphrena',
  'chasm',
] as const

// Public ----------------------------------------------------------------------

/** Resolves a project icon Lucide component by its identifier (`box` by default). */
export function resolveIconComponent(iconId?: string | null): typeof Box {
  return PROJECT_ICONS.find((entry) => entry.id === iconId)?.component ?? Box
}

/** Resolves a project swatch hex string by its color identifier (`null` for `default`). */
export function resolveColorHex(colorId?: string | null): string | null {
  return PROJECT_COLORS.find((entry) => entry.value === colorId)?.color ?? null
}
