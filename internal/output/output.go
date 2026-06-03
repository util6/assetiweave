package output

import (
	"encoding/json"
	"fmt"
	"io"
)

const (
	ExitValidation = 2
	ExitEngine     = 3
	ExitInternal   = 5
)

type Envelope struct {
	OK   bool `json:"ok"`
	Data any  `json:"data,omitempty"`
	Meta any  `json:"meta,omitempty"`
}

type ErrorEnvelope struct {
	OK    bool       `json:"ok"`
	Error *ErrDetail `json:"error"`
}

type ErrDetail struct {
	Type    string `json:"type"`
	Code    string `json:"code,omitempty"`
	Message string `json:"message"`
	Hint    string `json:"hint,omitempty"`
	Details any    `json:"details,omitempty"`
}

type ExitError struct {
	Code   int
	Detail *ErrDetail
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

func WriteSuccess(w io.Writer, data any) {
	PrintJSON(w, Envelope{OK: true, Data: data})
}

func WriteErrorEnvelope(w io.Writer, err *ExitError) {
	if err == nil || err.Detail == nil {
		err = Errorf(ExitInternal, "internal", "unknown error")
	}
	PrintJSON(w, ErrorEnvelope{OK: false, Error: err.Detail})
}
