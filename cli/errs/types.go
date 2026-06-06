package errs

import "fmt"

type TypedError interface {
	error
	ProblemDetail() *Problem
}

type base struct {
	problem Problem
	cause   error
}

func newBase(category Category, subtype Subtype, format string, args ...any) base {
	if !IsDeclaredSubtype(subtype) {
		panic(fmt.Sprintf("undeclared error subtype %q", subtype))
	}
	return base{
		problem: Problem{
			Category: category,
			Subtype:  subtype,
			Code:     string(subtype),
			Message:  formatMessage(format, args),
		},
	}
}

func (e *base) Error() string {
	if e == nil {
		return ""
	}
	return e.problem.Error()
}

func (e *base) ProblemDetail() *Problem {
	if e == nil {
		return nil
	}
	return &e.problem
}

func (e *base) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.cause
}

func (e *base) withCode(code string) {
	e.problem.Code = code
}

func (e *base) withWireType(wireType string) {
	e.problem.WireType = wireType
}

func (e *base) withHint(format string, args ...any) {
	e.problem.Hint = formatMessage(format, args)
}

func (e *base) withDetails(details any) {
	e.problem.Details = details
}

func (e *base) withMeta(meta any) {
	e.problem.Meta = meta
}

func (e *base) withCause(cause error) {
	e.cause = cause
}

type ValidationError struct {
	base
}

func NewValidationError(subtype Subtype, format string, args ...any) *ValidationError {
	return &ValidationError{base: newBase(CategoryValidation, subtype, format, args...)}
}

func (e *ValidationError) WithCode(code string) *ValidationError {
	e.withCode(code)
	return e
}

func (e *ValidationError) WithWireType(wireType string) *ValidationError {
	e.withWireType(wireType)
	return e
}

func (e *ValidationError) WithHint(format string, args ...any) *ValidationError {
	e.withHint(format, args...)
	return e
}

func (e *ValidationError) WithDetails(details any) *ValidationError {
	e.withDetails(details)
	return e
}

func (e *ValidationError) WithMeta(meta any) *ValidationError {
	e.withMeta(meta)
	return e
}

func (e *ValidationError) WithCause(cause error) *ValidationError {
	e.withCause(cause)
	return e
}

type ConfigError struct {
	base
}

func NewConfigError(subtype Subtype, format string, args ...any) *ConfigError {
	return &ConfigError{base: newBase(CategoryConfig, subtype, format, args...)}
}

func (e *ConfigError) WithCode(code string) *ConfigError {
	e.withCode(code)
	return e
}

func (e *ConfigError) WithWireType(wireType string) *ConfigError {
	e.withWireType(wireType)
	return e
}

func (e *ConfigError) WithHint(format string, args ...any) *ConfigError {
	e.withHint(format, args...)
	return e
}

func (e *ConfigError) WithDetails(details any) *ConfigError {
	e.withDetails(details)
	return e
}

func (e *ConfigError) WithMeta(meta any) *ConfigError {
	e.withMeta(meta)
	return e
}

func (e *ConfigError) WithCause(cause error) *ConfigError {
	e.withCause(cause)
	return e
}

type EngineError struct {
	base
}

func NewEngineError(subtype Subtype, format string, args ...any) *EngineError {
	return &EngineError{base: newBase(CategoryEngine, subtype, format, args...)}
}

func (e *EngineError) WithCode(code string) *EngineError {
	e.withCode(code)
	return e
}

func (e *EngineError) WithWireType(wireType string) *EngineError {
	e.withWireType(wireType)
	return e
}

func (e *EngineError) WithHint(format string, args ...any) *EngineError {
	e.withHint(format, args...)
	return e
}

func (e *EngineError) WithDetails(details any) *EngineError {
	e.withDetails(details)
	return e
}

func (e *EngineError) WithMeta(meta any) *EngineError {
	e.withMeta(meta)
	return e
}

func (e *EngineError) WithCause(cause error) *EngineError {
	e.withCause(cause)
	return e
}

type PolicyError struct {
	base
}

func NewPolicyError(subtype Subtype, format string, args ...any) *PolicyError {
	return &PolicyError{base: newBase(CategoryPolicy, subtype, format, args...)}
}

func (e *PolicyError) WithCode(code string) *PolicyError {
	e.withCode(code)
	return e
}

func (e *PolicyError) WithWireType(wireType string) *PolicyError {
	e.withWireType(wireType)
	return e
}

func (e *PolicyError) WithHint(format string, args ...any) *PolicyError {
	e.withHint(format, args...)
	return e
}

func (e *PolicyError) WithDetails(details any) *PolicyError {
	e.withDetails(details)
	return e
}

func (e *PolicyError) WithMeta(meta any) *PolicyError {
	e.withMeta(meta)
	return e
}

func (e *PolicyError) WithCause(cause error) *PolicyError {
	e.withCause(cause)
	return e
}

type InternalError struct {
	base
}

func NewInternalError(subtype Subtype, format string, args ...any) *InternalError {
	return &InternalError{base: newBase(CategoryInternal, subtype, format, args...)}
}

func (e *InternalError) WithCode(code string) *InternalError {
	e.withCode(code)
	return e
}

func (e *InternalError) WithWireType(wireType string) *InternalError {
	e.withWireType(wireType)
	return e
}

func (e *InternalError) WithHint(format string, args ...any) *InternalError {
	e.withHint(format, args...)
	return e
}

func (e *InternalError) WithDetails(details any) *InternalError {
	e.withDetails(details)
	return e
}

func (e *InternalError) WithMeta(meta any) *InternalError {
	e.withMeta(meta)
	return e
}

func (e *InternalError) WithCause(cause error) *InternalError {
	e.withCause(cause)
	return e
}

type ConfirmationRequiredError struct {
	base
}

func NewConfirmationRequiredError(format string, args ...any) *ConfirmationRequiredError {
	return &ConfirmationRequiredError{
		base: newBase(CategoryConfirmation, SubtypeConfirmationRequired, format, args...),
	}
}

func (e *ConfirmationRequiredError) WithCode(code string) *ConfirmationRequiredError {
	e.withCode(code)
	return e
}

func (e *ConfirmationRequiredError) WithWireType(wireType string) *ConfirmationRequiredError {
	e.withWireType(wireType)
	return e
}

func (e *ConfirmationRequiredError) WithHint(format string, args ...any) *ConfirmationRequiredError {
	e.withHint(format, args...)
	return e
}

func (e *ConfirmationRequiredError) WithDetails(details any) *ConfirmationRequiredError {
	e.withDetails(details)
	return e
}

func (e *ConfirmationRequiredError) WithMeta(meta any) *ConfirmationRequiredError {
	e.withMeta(meta)
	return e
}

func (e *ConfirmationRequiredError) WithCause(cause error) *ConfirmationRequiredError {
	e.withCause(cause)
	return e
}

func formatMessage(format string, args []any) string {
	if len(args) == 0 {
		return format
	}
	return fmt.Sprintf(format, args...)
}
