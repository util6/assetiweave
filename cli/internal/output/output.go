package output

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"

	"github.com/util6/assetiweave/errs"
)

const (
	ExitValidation           = 2
	ExitEngine               = 3
	ExitPolicy               = 6
	ExitConfirmationRequired = 10
	ExitInternal             = 5
)

type Envelope struct {
	OK     bool           `json:"ok"`
	Data   any            `json:"data,omitempty"`
	Meta   any            `json:"meta,omitempty"`
	Notice map[string]any `json:"_notice,omitempty"`
}

type ErrorEnvelope struct {
	OK     bool           `json:"ok"`
	Error  *ErrDetail     `json:"error"`
	Meta   any            `json:"meta,omitempty"`
	Notice map[string]any `json:"_notice,omitempty"`
}

type ErrDetail struct {
	Type    string `json:"type"`
	Subtype string `json:"subtype,omitempty"`
	Code    string `json:"code,omitempty"`
	Message string `json:"message"`
	Hint    string `json:"hint,omitempty"`
	Details any    `json:"details,omitempty"`
}

type ExitError struct {
	Code   int
	Detail *ErrDetail
	Meta   any
	Err    error
}

func (e *ExitError) Error() string {
	if e.Detail != nil {
		return e.Detail.Message
	}
	if e.Err != nil {
		return e.Err.Error()
	}
	return fmt.Sprintf("exit %d", e.Code)
}

func (e *ExitError) Unwrap() error {
	return e.Err
}

func Errorf(code int, errType, format string, args ...any) *ExitError {
	return &ExitError{
		Code: code,
		Detail: &ErrDetail{
			Type:    errType,
			Code:    errType,
			Message: fmt.Sprintf(format, args...),
		},
	}
}

func ErrWithHint(code int, errType, message, hint string) *ExitError {
	return &ExitError{
		Code: code,
		Detail: &ErrDetail{
			Type:    errType,
			Code:    errType,
			Message: message,
			Hint:    hint,
		},
	}
}

func PrintJSON(w io.Writer, value any) {
	encoder := json.NewEncoder(w)
	encoder.SetEscapeHTML(false)
	encoder.SetIndent("", "  ")
	_ = encoder.Encode(value)
}

var PendingNotice func() map[string]any

func GetNotice() map[string]any {
	if PendingNotice == nil {
		return nil
	}
	return PendingNotice()
}

func WriteSuccess(w io.Writer, data any) {
	PrintJSON(w, Envelope{OK: true, Data: data, Notice: GetNotice()})
}

func WriteSuccessWithMeta(w io.Writer, data, meta any) {
	PrintJSON(w, Envelope{OK: true, Data: data, Meta: meta, Notice: GetNotice()})
}

func WriteErrorEnvelope(w io.Writer, err *ExitError) {
	if err == nil || err.Detail == nil {
		err = Errorf(ExitInternal, "internal", "unknown error")
	}
	PrintJSON(w, ErrorEnvelope{OK: false, Error: err.Detail, Meta: err.Meta, Notice: GetNotice()})
}

func ExitCodeForCategory(category errs.Category) int {
	switch category {
	case errs.CategoryValidation, errs.CategoryConfig:
		return ExitValidation
	case errs.CategoryEngine:
		return ExitEngine
	case errs.CategoryPolicy:
		return ExitPolicy
	case errs.CategoryConfirmation:
		return ExitConfirmationRequired
	case errs.CategoryInternal:
		return ExitInternal
	default:
		return ExitInternal
	}
}

func ExitCodeOf(err error) int {
	if err == nil {
		return 0
	}
	if problem, ok := errs.ProblemOf(err); ok {
		return ExitCodeForCategory(problem.Category)
	}
	var exitErr *ExitError
	if errors.As(err, &exitErr) {
		return exitErr.Code
	}
	return ExitInternal
}

func WriteTypedErrorEnvelope(w io.Writer, err error) bool {
	problem, ok := errs.ProblemOf(err)
	if !ok {
		return false
	}
	wireType := string(problem.Category)
	if problem.WireType != "" {
		wireType = problem.WireType
	}
	detail := &ErrDetail{
		Type:    wireType,
		Subtype: string(problem.Subtype),
		Code:    problem.Code,
		Message: problem.Message,
		Hint:    problem.Hint,
		Details: problem.Details,
	}
	if detail.Code == "" {
		detail.Code = string(problem.Subtype)
	}
	var buffer bytes.Buffer
	encoder := json.NewEncoder(&buffer)
	encoder.SetEscapeHTML(false)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(ErrorEnvelope{OK: false, Error: detail, Meta: problem.Meta, Notice: GetNotice()}); err != nil {
		return false
	}
	_, _ = w.Write(buffer.Bytes())
	return true
}
