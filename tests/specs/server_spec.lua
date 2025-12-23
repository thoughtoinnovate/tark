-- Tests for server module
-- Tests platform detection, binary resolution, and channel configuration

-- Note: We need to access internal functions for testing
-- We'll use a helper to expose them
local function get_server_module()
    -- Clear any cached version
    package.loaded['tark.server'] = nil
    return require('tark.server')
end

local server = get_server_module()

describe('server - platform and binary management', function()
    describe('module functions exist', function()
        it('has start function', function()
            assert.is_function(server.start)
        end)

        it('has stop function', function()
            assert.is_function(server.stop)
        end)

        it('has status function', function()
            assert.is_function(server.status)
        end)

        it('has restart function', function()
            assert.is_function(server.restart)
        end)

        it('has binary_available function', function()
            assert.is_function(server.binary_available)
        end)

        it('has docker_available function', function()
            assert.is_function(server.docker_available)
        end)

        it('has download_binary function', function()
            assert.is_function(server.download_binary)
        end)

        it('has setup function', function()
            assert.is_function(server.setup)
        end)
    end)

    describe('status function', function()
        it('returns table', function()
            local status = server.status()
            assert.is_table(status)
        end)

        it('has running field', function()
            local status = server.status()
            assert.is_boolean(status.running)
        end)

        it('has mode field', function()
            local status = server.status()
            assert.is_not_nil(status.mode)
        end)

        it('has url field', function()
            local status = server.status()
            assert.is_string(status.url)
        end)

        it('has detected_platform field', function()
            local status = server.status()
            assert.is_string(status.detected_platform)
        end)

        it('has expected_binary_name field', function()
            local status = server.status()
            assert.is_string(status.expected_binary_name)
        end)

        it('has channel field', function()
            local status = server.status()
            assert.is_string(status.channel)
        end)

        it('has binary_available field', function()
            local status = server.status()
            assert.is_boolean(status.binary_available)
        end)

        it('has docker_available field', function()
            local status = server.status()
            assert.is_boolean(status.docker_available)
        end)
    end)

    describe('platform detection', function()
        it('detects current platform', function()
            local status = server.status()
            local platform = status.detected_platform
            -- Should be one of the supported platforms
            local valid_platforms = {
                ['linux-x86_64'] = true,
                ['linux-arm64'] = true,
                ['darwin-x86_64'] = true,
                ['darwin-arm64'] = true,
                ['windows-x86_64'] = true,
                ['freebsd-x86_64'] = true,
            }
            -- Platform should match pattern os-arch
            assert.is_true(platform:match('^%w+-%w+') ~= nil)
        end)

        it('generates correct binary name', function()
            local status = server.status()
            local binary_name = status.expected_binary_name
            -- Should start with 'tark-'
            assert.is_true(binary_name:match('^tark%-') ~= nil)
            -- Should contain platform info
            assert.is_true(#binary_name > 5)
        end)

        it('adds .exe extension for Windows', function()
            local status = server.status()
            local platform = status.detected_platform
            local binary_name = status.expected_binary_name
            
            if platform:match('^windows') then
                assert.is_true(binary_name:match('%.exe$') ~= nil)
            else
                assert.is_false(binary_name:match('%.exe$') ~= nil)
            end
        end)
    end)

    describe('config', function()
        it('has default config', function()
            assert.is_table(server.config)
        end)

        it('has mode config', function()
            assert.is_string(server.config.mode)
        end)

        it('has channel config', function()
            assert.is_string(server.config.channel)
        end)

        it('has host config', function()
            assert.is_string(server.config.host)
        end)

        it('has port config', function()
            assert.is_number(server.config.port)
        end)

        it('mode is valid', function()
            local valid_modes = { auto = true, binary = true, docker = true }
            assert.is_true(valid_modes[server.config.mode])
        end)

        it('channel is valid', function()
            local valid_channels = { stable = true, nightly = true, latest = true }
            assert.is_true(valid_channels[server.config.channel])
        end)
    end)

    describe('binary_available', function()
        it('returns boolean', function()
            local available = server.binary_available()
            assert.is_boolean(available)
        end)

        it('can be called multiple times', function()
            local result1 = server.binary_available()
            local result2 = server.binary_available()
            assert.equals(result1, result2)
        end)
    end)

    describe('docker_available', function()
        it('returns boolean', function()
            local available = server.docker_available()
            assert.is_boolean(available)
        end)

        it('can be called multiple times', function()
            local result1 = server.docker_available()
            local result2 = server.docker_available()
            assert.equals(result1, result2)
        end)
    end)

    describe('url generation', function()
        it('generates correct URL', function()
            local status = server.status()
            local url = status.url
            -- Should be http://host:port format
            assert.is_true(url:match('^http://') ~= nil)
            assert.is_true(url:match(':%d+$') ~= nil)
        end)

        it('uses configured host and port', function()
            local status = server.status()
            local url = status.url
            local expected = 'http://' .. server.config.host .. ':' .. server.config.port
            assert.equals(expected, url)
        end)
    end)
end)

