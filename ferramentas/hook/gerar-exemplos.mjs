// Regrava os JSON de exemplo do hook (o formato que o Claude Code manda no stdin).
// Uso: node ferramentas/hook/gerar-exemplos.mjs
import { writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const aqui = dirname(fileURLToPath(import.meta.url))
const base = { session_id: '00000000-0000-4000-8000-000000000001', transcript_path: 'ferramentas/hook/exemplo-transcript.jsonl', cwd: 'A:\\Claude\\03-projetos\\curso-mirian' }
const exemplos = {
  'exemplo-prompt.json': { ...base, hook_event_name: 'UserPromptSubmit', prompt: 'Sara, faz a landing do curso da Mirian com duas propostas de visual.' },
  'exemplo-pretool.json': { ...base, hook_event_name: 'PreToolUse', tool_name: 'Read', tool_input: { file_path: 'A:\\Claude\\03-projetos\\curso-mirian\\LEIA-ME.md' } },
  'exemplo-notification.json': { ...base, hook_event_name: 'Notification', message: 'Claude needs your permission to use Bash' },
  'exemplo-subagentstop.json': { ...base, hook_event_name: 'SubagentStop', stop_hook_active: false },
  'exemplo-stop.json': { ...base, hook_event_name: 'Stop', stop_hook_active: false },
}
for (const [nome, obj] of Object.entries(exemplos)) writeFileSync(join(aqui, nome), JSON.stringify(obj, null, 2) + '\n')
console.log('exemplos regravados:', Object.keys(exemplos).join(', '))
