describe("When services own reactive review data", function()
  -- fixtures ------------------------------------------------------------------

  local threads = require("lgtm.services.threads_service")
  local files = require("lgtm.services.files_service")
  local rpc = require("lgtm.services.client")
  local saved_call

  before_each(function()
    saved_call = rpc.call
  end)

  after_each(function()
    rpc.call = saved_call
  end)

  -- tests ---------------------------------------------------------------------

  it("Publishes conversation and placements together and rejects component writes", function()
    local context, snapshots = {}, {}
    local observer = threads.get_data(context):get_observable():subscribe(function(value)
      snapshots[#snapshots + 1] = value
    end)
    local conversation = { threads = { { id = "thread" } }, comments = {} }
    local placements = { thread = { new = { status = "outdated" } } }

    threads.publish(context, conversation, placements, {})

    assert.equal(conversation, context.conversation)
    assert.equal(placements, snapshots[#snapshots].placements)
    assert.has_error(function()
      context.conversation = {}
    end, "Conversation data is owned by threads_service")
    observer:unsubscribe()
    local count = #snapshots
    threads.publish(context, { threads = {}, comments = {} }, {}, {})
    assert.equal(count, #snapshots)
  end)

  it("Keeps review contexts independent and exposes separate derived subscriptions", function()
    local first, second = {}, {}
    local comments, thread_count
    local one = threads
      .get_data(first)
      :map(function(data)
        return data.conversation and #data.conversation.comments or 0
      end)
      :get_observable()
      :subscribe(function(value)
        comments = value
      end)
    local two = threads
      .get_data(first)
      :map(function(data)
        return data.conversation and #data.conversation.threads or 0
      end)
      :get_observable()
      :subscribe(function(value)
        thread_count = value
      end)
    threads.get_data(second)

    threads.publish(first, { threads = { {} }, comments = { {}, {} } }, {}, {})

    assert.equal(2, comments)
    assert.equal(1, thread_count)
    assert.is_nil(second.conversation)
    one:unsubscribe()
    two:unsubscribe()
  end)

  it("Publishes an optimistic reviewed mark and rolls it back after a rejected write", function()
    local context = { review = { id = "review" } }
    local accepted, observed = nil, {}
    rawset(rpc, "call", function(method, _, callback)
      assert.equal("SetFileReviewed", method)
      accepted = callback
    end)
    local subscription = files.get_data(context):get_observable():subscribe(function(data)
      observed[#observed + 1] = data.reviewed["file.md"] or false
    end)

    files.toggle_reviewed(context, { path = "file.md", contentHash = "source-identity" })
    assert.equal("source-identity", context.reviewed["file.md"])
    if not accepted then
      error("Reviewed-mark mutation was not sent")
    end
    accepted({ message = "Rejected" })

    assert.is_nil(context.reviewed["file.md"])
    assert.same({ false, "source-identity", false }, observed)
    assert.has_error(function()
      context.files = {}
    end, "File data is owned by files_service")
    subscription:unsubscribe()
  end)
end)
