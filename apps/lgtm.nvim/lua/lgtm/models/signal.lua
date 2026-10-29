---The NuiComponents reactive API used by services. These declarations contain no runtime implementation.
---@class NuiSignalValue
---@field get_value fun(self: NuiSignalValue): any
---@field map fun(self: NuiSignalValue, transform: function): NuiSignalValue
---@field combine_latest fun(self: NuiSignalValue, ...): NuiSignalValue
---@field get_observable fun(self: NuiSignalValue): NuiObservable

---@class NuiObservable
---@field subscribe fun(self: NuiObservable, observer: fun(value: any)): NuiSubscription

---@class NuiSubscription
---@field unsubscribe fun(self: NuiSubscription)
