// node --test: as regras puras do sidecar (sem abrir o SDK).
import test from 'node:test'
import assert from 'node:assert/strict'
import { sugestoesSeguras } from '../../sidecar/regras.mjs'

test('lembrar aprovações nunca troca o modo nem grava nos settings', () => {
  const regra = { toolName: 'Bash', ruleContent: 'npm test' }
  const sugestoes = [
    { type: 'setMode', mode: 'acceptEdits', destination: 'session' },
    { type: 'setMode', mode: 'bypassPermissions', destination: 'session' },
    { type: 'addRules', behavior: 'allow', destination: 'userSettings', rules: [regra] },
    { type: 'addRules', behavior: 'allow', destination: 'projectSettings', rules: [regra] },
    { type: 'addRules', behavior: 'allow', destination: 'localSettings', rules: [regra] },
    { type: 'addDirectories', directories: ['C:/'], destination: 'session' },
    { type: 'addRules', behavior: 'deny', destination: 'session', rules: [regra] },
    { type: 'addRules', behavior: 'allow', destination: 'session', rules: [] },
    { type: 'addRules', behavior: 'allow', destination: 'session', rules: [regra] },
  ]
  assert.deepEqual(sugestoesSeguras(sugestoes), [sugestoes[8]])
  assert.deepEqual(sugestoesSeguras(undefined), [])
  assert.deepEqual(sugestoesSeguras('x'), [])
})
