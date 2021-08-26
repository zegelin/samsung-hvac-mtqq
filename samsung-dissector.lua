-- dissector for Samsung HVAC Wired Remote Control <-> Indoor Unit RS485 comms
local samsung_hvac_wrc = Proto("samsung_hvac_wrc", "Samsung RS485 HVAC WRC Protocol")

-- from manual: RMC address: indoor unit group address

local ADDRESSES = {
    [0x20] = "Indoor unit",  -- main remote polls 0x20 -> 0x6f (80 addresses) on boot. manual says max 16 devices though.
    [0x84] = "Main remote",
    [0x85] = "Sub remote", -- ??
    [0xAD] = "Broadcast?"
    -- 0xc9
    -- 0xeb
    -- 0x
}

for i = 0, 0x6f-0x20 do
    ADDRESSES[0x20+i] = string.format("Indoor unit %d", i+1)
end


local COMMANDS = {
    [0xA0] = "Change settings", -- request
    [0x50] = "Change settings", -- reply
    [0x52] = "Information", -- request/reply
    [0x53] = "Information", -- request/reply
    [0x54] = "Information", -- request/reply
    -- 0x63
    [0x64] = "Temperature", -- request/reply

    -- 0x70
    -- 0x71

    -- 0x83

    -- 0xc5 (main remote (0x84) -> sub remote? (0x85))
    -- 0xc4 (main remote (0x84) -> sub remote? (0x85))
    -- 0xc6 (main remote (0x84) -> 0xc9)
    -- 0xc4 (main remote (0x84) -> 0xc9)

    -- 0xd1 (main remote (0x84) -> broadcast (0xad)
}

local FAN_SPEEDS = {
    [0x0] = 'fan auto',
    [0x2] = 'fan low',
    [0x4] = 'fan medium',
    [0x5] = 'fan high'
}

local MODES = {
    [0x0] = 'auto',
    [0x1] = 'cool',
    [0x2] = 'dry',
    [0x3] = 'fan',
    [0x4] = 'heat'
}

local BLADE_POSITIONS = {
    [0x0] = 'blade closed',
    [0x1] = 'blade open smallest',
    [0x2] = 'blade open midway',
    [0x7] = 'blade open max'
}

local fields = {
        src_addr = ProtoField.uint8("samsung_hvac_wrc.src_addr", "Source Address", base.HEX_DEC, ADDRESSES),
        dst_addr = ProtoField.uint8("samsung_hvac_wrc.dst_addr", "Destination Address", base.HEX_DEC, ADDRESSES),
        command = ProtoField.uint8("samsung_hvac_wrc.cmd", "Command", base.HEX_DEC, COMMANDS),
        data = ProtoField.bytes("samsung_hvac_wrc.data", "Command Data"),
        checksum = ProtoField.uint8("samsung_hvac_wrc.cksum", "Checksum", base.HEX_DEC),
        test = ProtoField.framenum("samsung_hvac_wrc.frame", "Foo", base.NONE, frametype.REQUEST)
}

samsung_hvac_wrc.fields = fields

local experts = {
    too_short = ProtoExpert.new("samsung_hvac_wrc.too_short.expert", "Packet too short",
                                     expert.group.MALFORMED, expert.severity.ERROR),
    framing_error = ProtoExpert.new("samsung_hvac_wrc.framing_error.expert", "Packet framing error",
                                     expert.group.MALFORMED, expert.severity.ERROR),
    checksum_invalid = ProtoExpert.new("samsung_hvac_wrc.checksum_invalid.expert", "Invalid checksum",
                                     expert.group.CHECKSUM, expert.severity.ERROR),
    const_assert = ProtoExpert.new("samsung_hvac_wrc.const_assert.expert", "Constant assertion failed",
                                     expert.group.MALFORMED, expert.severity.ERROR)
}

samsung_hvac_wrc.experts = experts




function samsung_hvac_wrc.dissector(buf, pktinfo, root)
    pktinfo.cols.protocol:set("Samsung RS485 HVAC WRC")

    local tree = root:add(samsung_hvac_wrc, buf())

    if buf(0, 1):uint() ~= 0x32 or buf(buf:len()-1, 1):uint() ~= 0x34 then
        tree:add_proto_expert_info(experts.framing_error, "Start and/or end marker mismatch (not 0x32/0x34)")
        return
    end

    local checksum_r = buf(buf:len()-2, 1)
    local payload = buf(1, buf:len()-3):tvb()

    if payload:len() < 3 then
        tree:add_proto_expert_info(experts.too_short)
        return
    end

    local src_address_r = payload(0, 1)
    local src_address = src_address_r:uint()
    tree:add(fields.src_addr, src_address_r)

    local is_reply = 0x20 <= src_address and src_address <= 0x64

    local dst_address_r = payload(1, 1)
    local dst_address = dst_address_r:uint()
    tree:add(fields.dst_addr, dst_address_r)

    local command_r = payload(2, 1)
    local command = command_r:uint()
    tree:add(fields.command, command_r)

    pktinfo.cols.info:set(string.format(
            '%s (0x%x) -> %s (0x%x): %s (0x%x) %s',
            ADDRESSES[src_address] or "Unknown", src_address,
            ADDRESSES[dst_address] or "Unknown", dst_address,
            COMMANDS[command] or "Unknown command", command,
            is_reply and "reply" or "request"
    ))

    local has_extra_info = false
    function add_extra_info(s)
        if not has_extra_info then
            pktinfo.cols.info:append(' (' .. s)
        else
            pktinfo.cols.info:append(', ' .. s)
        end

        has_extra_info = true
    end

    local data_r = payload(3)
    local data_t = tree:add(fields.data, data_r)

    local checksum_t = tree:add(fields.checksum, checksum_r)
    do
        -- validate chechsum
        local checksummed_s = payload:raw()
        local new_checksum = string.byte(checksummed_s, 1)

        for i = 2, string.len(checksummed_s) do
            new_checksum = bit32.bxor(new_checksum, string.byte(checksummed_s, i))
        end

        if checksum_r:uint() ~= new_checksum then
            checksum_t:add_proto_expert_info(experts.checksum_invalid, "" .. new_checksum)
            checksum_t:append_text(" [invalid]")
            return
        else
            checksum_t:append_text(" [valid]")
        end
    end

    function data_bytes(start, length)
        length = length or 1
        return data_r:range(start-1, length)
    end

    function bits(i, start_bit, end_bit)
        -- get bits [start, end]. MSB 0
        -- returns a nibble-chunked bit string (for tree view) + integer value of bits
        local s = ''
        local bytes = data_bytes(i)
        local v = bytes:uint()
        for i = 0, 7 do
            if i < start_bit or i > end_bit then
                s = s .. '.'
            else
                s = s .. bit32.rshift(bit32.band(v, 0x80), 7)
            end

            if i == 3 then
                s = s .. ' '
            end

            v = bit32.lshift(v, 1)
        end

        return s, bytes:bitfield(start_bit, 1 + end_bit - start_bit)  -- MSB 0
    end

    function data_node(i, format, ...)
        return data_t:add(data_bytes(i), string.format('%d: ' .. format, i, ...))
    end

    function data_node_unknown(i, start_bit, end_bit)
        local bit_str, value = bits(i, start_bit, end_bit)
        return data_node(i, '%s = Unknown (0x%x)', bit_str, value), value
    end

    function data_node_constant(i, start_bit, end_bit, c)
        local bit_str, value = bits(i, start_bit, end_bit)
        if value == c then
            return data_node(i, '%s = const 0x%x', bit_str, value), value
        else
            local node_t = data_node(i, '%s = 0x%x [expected 0x%x]', bit_str, value, c)
            node_t:add_proto_expert_info(experts.const_assert)
            return node_t, value
        end
    end

    function data_node_table(i, start_bit, end_bit, name, lookup)
        local bit_str, value = bits(i, start_bit, end_bit)
        local lvalue = lookup[value] or "unknown"
        add_extra_info(lvalue)
        return data_node(i, '%s = %s: %s (0x%x)', bit_str, name, lvalue, value), lvalue
    end

    function data_node_bool(i, bit, name, fv, tv)
        return data_node_table(i, bit, bit, name, {
            [0x0] = fv or "false",
            [0x1] = tv or "true"
        })
    end

    function data_node_int(i, start_bit, end_bit, name, add_extra)
        local bit_str, value = bits(i, start_bit, end_bit)
        if add_extra == nil and true or add_extra then
            add_extra_info(string.format('%d', value))
        end
        return data_node(i, '%s = %s: %d (0x%x)', bit_str, name, value, value), value
    end

    function data_node_temperature(i, start_bit, end_bit, name, offset)
        offset = offset or 0
        local bit_str, raw_temp = bits(i, start_bit, end_bit)
        local offset_temp = raw_temp + offset
        add_extra_info(string.format('%d°C', offset_temp))
        return data_node(i, '%s = %s: %d°C (%d+%d, 0x%x)', bit_str, name, offset_temp, raw_temp, offset, raw_temp), offset_temp
    end

    if command == 0xA0 or command == 0x50 then
        data_node_unknown(1, 0, 1)
        data_node_bool(1, 2, 'Set Sleep', 'normal', 'sleep')
        data_node_table(1, 3, 7, 'Set Blade', {
            [0x1A] = 'swing up/down',
            [0x1F] = 'swing off'
        })

        data_node_unknown(2, 0, 7)

        data_node_table(3, 0, 2, 'Set Fan Speed', FAN_SPEEDS)
        data_node_temperature(3, 3, 7, 'Set Temperature')

        data_node_unknown(4, 0, 1)
        data_node_bool(4, 2, 'Reset Clean Filter')
        data_node_unknown(4, 3, 4)
        data_node_table(4, 5, 7, 'Set Mode', MODES)

        data_node_table(5, 0, 7, 'Set Power', {
            [0xc4] = 'off',
            [0xf4] = 'on'
        })

        data_node_constant(6, 0, 7, 0x00)

        data_node_unknown(7, 0, 1)
        data_node_bool(7, 2, 'Set Quiet Mode', 'normal', 'quiet')
        data_node_bool(7, 3, 'Set Blade Position')
        data_node_table(7, 4, 7, 'Blade Position', BLADE_POSITIONS)

        data_node_constant(8, 0, 7, 0x00)

    elseif command == 0x52 and is_reply then
        data_node_unknown(1, 0, 2)
        data_node_temperature(1, 3, 7, 'Setpoint Temperature', 9)

        data_node_unknown(2, 0, 2)
        data_node_temperature(2, 3, 7, 'Discharge Temperature?', 9)

        data_node_unknown(3, 0, 7)
        data_node_temperature(3, 3, 7, 'Temperature?', 9)


        data_node_table(4, 0, 4, 'Blade', {
            [0x1A] = 'swing up/down',
            [0x1F] = 'swing off',
        })
        data_node_table(4, 5, 7, 'Fan Speed', FAN_SPEEDS)

        data_node_bool(5, 0, 'Power', 'power off', 'power on')
        data_node_unknown(5, 1, 2)
        data_node_bool(5, 3, 'Defroster', 'defroster off', 'defroster on')
        data_node_table(5, 4, 7, 'Remote Type', {
            [0x1] = "wired",
            [0x2] = "remote"
        })

        data_node_unknown(6, 0, 2)
        data_node_bool(6, 3, 'Filter:', 'filter clean', 'filter dirty')
        data_node_unknown(6, 4, 7)

        data_node_constant(7, 0, 7, 0x00)

        data_node_unknown(8, 0, 7)
        data_node_temperature(8, 3, 7, 'Temperature?', 9)

    elseif command == 0x53 and is_reply then
        data_node_constant(1, 0, 7, 0x00)

        data_node_constant(2, 0, 7, 0x00)

        data_node_constant(3, 0, 7, 0x00)

        data_node_constant(4, 0, 7, 0x00)

        data_node_table(5, 0, 7, 'Blade', {
            [0x0] = 'swing off',
            [0x1a] = 'swing up/down'
        })

        data_node_constant(6, 0, 7, 0x00)

        data_node_constant(7, 0, 7, 0x00)

        data_node_unknown(8, 0, 4)
        data_node_table(8, 5, 7, 'Mode', MODES)

    elseif command == 0x54 and is_reply then
        data_node_unknown(1, 0, 7)

        data_node_unknown(2, 0, 3)
        data_node_table(2, 4, 7, 'Blade Position', BLADE_POSITIONS)

        data_node_unknown(3, 0, 7)

        data_node_unknown(4, 0, 7)

        data_node_unknown(5, 0, 7)

        data_node_unknown(6, 0, 7)

        data_node_constant(7, 0, 7, 0x00)

        data_node_constant(8, 0, 7, 0x00)

    elseif command == 0x64 then
        data_node_unknown(1, 0, 7)

        data_node_unknown(2, 0, 6)
        data_node_table(2, 7, 7, 'Temperature Probe Source', {
            [0x00] = 'indoor unit sensor',
            [0x01] = 'wired remote sensor'
        })

        function temp(b1, b2, name)
            local high_byte = select(2, data_node_int(b1, 0, 7, name .. ' (high byte)', false))
            local low_byte = select(2, data_node_int(b2, 0, 7, name .. ' (low byte)', false))
            local temp = (((high_byte * 256) + low_byte) - 553.0) / 10.0
            local temp_str = string.format('%.2f°C', temp)

            data_t:add(data_bytes(3, 2), string.format('%s = %s', string.rep(' ', 12), temp_str))
            add_extra_info(temp_str)
        end

        temp(3, 4, is_reply and 'Temperature Probe' or 'Wired Remote Temperature Probe')

        if is_reply then
            temp(5, 6, 'Indoor Unit Temperature Probe')
        else
            data_node_constant(5, 0, 7, 0x00)
            data_node_constant(6, 0, 7, 0x00)
        end

        data_node_constant(7, 0, 7, 0x00)

        data_node_constant(8, 0, 7, 0x00)
    end

    if has_extra_info then
        pktinfo.cols.info:append(')')
    end


end


DissectorTable.get("udp.port"):add(45654, samsung_hvac_wrc)
