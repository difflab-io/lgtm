/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { access } from 'node:fs/promises'
import { usePluginInstallation } from './install.fixtures.js'

describe('When installing LGTM through one lazy.nvim declaration', { tags: ['required'] }, () => {
  const fixture = usePluginInstallation()

  it('should_install_required_plugins_from_one_lgtm_declaration', async () => {
    // Arrange
    const installation = fixture()
    expect(
      await access(installation.managerRoot).then(
        () => true,
        () => false,
      ),
    ).toBe(false)

    // Act
    const installed = await installation.start()

    // Assert
    expect(installed.composer_open).toBe(true)
    expect(installed.plugins).toEqual([
      'lgtm',
      'nui.nvim',
      'nui-components.nvim',
      'codediff.nvim',
      'which-key.nvim',
      'nvim-notify',
    ])

    // Arrange
    // The same manager root now contains the installed plugin and its package metadata.

    // Act
    const restarted = await installation.start({ customizeProviders: true })

    // Assert
    expect(restarted.composer_open).toBe(true)
    expect(restarted.customized).toBe(true)
    expect(restarted.plugins).toEqual(installed.plugins)
  })
})

describe('When switching the installed LGTM source', { tags: ['required'] }, () => {
  const fixture = usePluginInstallation()

  it('should_load_the_development_plugin_with_an_existing_package_cache', async () => {
    // Arrange
    const installation = fixture()

    // Act
    const installed = await installation.start({ source: 'development' })

    // Assert
    expect(installed.composer_open).toBe(true)
  })

  it('should_load_the_released_plugin_without_package_metadata', async () => {
    // Arrange
    const installation = fixture()

    // Act
    const installed = await installation.start({ source: 'released' })

    // Assert
    expect(installed.composer_open).toBe(true)
  })
})
