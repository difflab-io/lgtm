-- tests -----------------------------------------------------------------------
local function should_suppress_cancelled_transport_responses(H)
	local transport = require("lgtm.transport.connect")
	local callbacks = 0
	local cancel = transport.call("IdentityService", "GetCapabilities", {}, nil, function()
		callbacks = callbacks + 1
	end)
	cancel()
	transport.call("IdentityService", "GetCapabilities", {}, nil, function()
		callbacks = callbacks + 1
	end)
	transport.call("IdentityService", "GetCapabilities", {}, nil, function()
		callbacks = callbacks + 1
	end)
	transport.cancel_all()
	local elapsed = false
	vim.defer_fn(function()
		elapsed = true
	end, 250)
	H.wait(function()
		return elapsed
	end, "cancelled real curl subprocesses settle")
	assert(callbacks == 0, "individual and global cancellation suppress response/error callbacks")
	local successful
	transport.call("IdentityService", "GetCapabilities", {}, nil, function(err, result)
		assert(not err)
		successful = result.apiVersion
	end)
	H.wait(function()
		return successful ~= nil
	end, "transport remains usable after cancellation")
	assert(successful == "1")
end

return should_suppress_cancelled_transport_responses
