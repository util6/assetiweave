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

func formatMessage(format string, args []any) string {
	if len(args) == 0 {
		return format
	}
	return fmt.Sprintf(format, args...)
}
