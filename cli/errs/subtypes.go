package errs

import "sort"

type Subtype string

const (
	SubtypeUnknown Subtype = "unknown"
)

const (
	SubtypeInvalidArgument     Subtype = "invalid_argument"
	SubtypeInvalidJSON         Subtype = "invalid_json"
	SubtypeInvalidParams       Subtype = "invalid_params"
	SubtypeUnknownMethod       Subtype = "unknown_method"
	SubtypeUnknownCommand      Subtype = "unknown_command"
	SubtypeUnknownFlag         Subtype = "unknown_flag"
	SubtypeMissingSubcommand   Subtype = "missing_subcommand"
	SubtypeMissingRequiredFlag Subtype = "missing_required_flag"
	SubtypeFlagError           Subtype = "flag_error"
	SubtypeHookAborted         Subtype = "hook_aborted"
)

const (
	SubtypeInvalidConfig         Subtype = "invalid_config"
	SubtypeInvalidPluginConfig   Subtype = "invalid_plugin_config"
	SubtypePluginInstallFailed   Subtype = "plugin_install_failed"
	SubtypePluginLifecycleFailed Subtype = "plugin_lifecycle_failed"
)

const (
	SubtypeEngineNotFound      Subtype = "engine_not_found"
	SubtypeEngineProcess       Subtype = "engine_process"
	SubtypeEngineProtocol      Subtype = "engine_protocol"
	SubtypeEngineIncompatible  Subtype = "engine_incompatible"
	SubtypeEngineInternal      Subtype = "engine_internal"
	SubtypeEngineReturnedError Subtype = "engine_returned_error"
	SubtypeNotFound            Subtype = "not_found"
	SubtypeConflict            Subtype = "conflict"
	SubtypeOperationError      Subtype = "operation_error"
)

const (
	SubtypeCommandDenied        Subtype = "command_denied"
	SubtypePolicyInvalid        Subtype = "policy_invalid"
	SubtypePluginPolicyConflict Subtype = "plugin_policy_conflict"
)

const (
	SubtypeConfirmationRequired Subtype = "confirmation_required"
)

const (
	SubtypeHookPanic Subtype = "hook_panic"
)

const (
	SubtypeUpdateFailed Subtype = "update_failed"
)

var declaredSubtypes = map[Subtype]struct{}{
	SubtypeUnknown:               {},
	SubtypeInvalidArgument:       {},
	SubtypeInvalidJSON:           {},
	SubtypeInvalidParams:         {},
	SubtypeUnknownMethod:         {},
	SubtypeUnknownCommand:        {},
	SubtypeUnknownFlag:           {},
	SubtypeMissingSubcommand:     {},
	SubtypeMissingRequiredFlag:   {},
	SubtypeFlagError:             {},
	SubtypeHookAborted:           {},
	SubtypeInvalidConfig:         {},
	SubtypeInvalidPluginConfig:   {},
	SubtypePluginInstallFailed:   {},
	SubtypePluginLifecycleFailed: {},
	SubtypeEngineNotFound:        {},
	SubtypeEngineProcess:         {},
	SubtypeEngineProtocol:        {},
	SubtypeEngineIncompatible:    {},
	SubtypeEngineInternal:        {},
	SubtypeEngineReturnedError:   {},
	SubtypeNotFound:              {},
	SubtypeConflict:              {},
	SubtypeOperationError:        {},
	SubtypeCommandDenied:         {},
	SubtypePolicyInvalid:         {},
	SubtypePluginPolicyConflict:  {},
	SubtypeConfirmationRequired:  {},
	SubtypeHookPanic:             {},
	SubtypeUpdateFailed:          {},
}

func DeclaredSubtypes() []Subtype {
	subtypes := make([]Subtype, 0, len(declaredSubtypes))
	for subtype := range declaredSubtypes {
		subtypes = append(subtypes, subtype)
	}
	sort.Slice(subtypes, func(i, j int) bool {
		return subtypes[i] < subtypes[j]
	})
	return subtypes
}

func IsDeclaredSubtype(subtype Subtype) bool {
	_, ok := declaredSubtypes[subtype]
	return ok
}
