package errs

type Category string

const (
	CategoryValidation   Category = "validation"
	CategoryConfig       Category = "config"
	CategoryEngine       Category = "engine"
	CategoryPolicy       Category = "policy"
	CategoryInternal     Category = "internal"
	CategoryConfirmation Category = "confirmation"
)
