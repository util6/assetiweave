package suggest

import "sort"

// Closest returns plausible candidates ordered by shared prefix, edit
// distance, and name. The ordering is stable for agent-facing error details.
func Closest(typed string, candidates []string, maxResults int) []string {
	type scored struct {
		name   string
		prefix int
		dist   int
	}

	limit := editLimit(typed)
	ranked := make([]scored, 0, len(candidates))
	for _, candidate := range candidates {
		prefix := sharedPrefixLength(typed, candidate)
		distance := levenshtein(typed, candidate)
		if prefix >= 3 || distance <= limit {
			ranked = append(ranked, scored{
				name:   candidate,
				prefix: prefix,
				dist:   distance,
			})
		}
	}
	sort.Slice(ranked, func(left, right int) bool {
		if ranked[left].prefix != ranked[right].prefix {
			return ranked[left].prefix > ranked[right].prefix
		}
		if ranked[left].dist != ranked[right].dist {
			return ranked[left].dist < ranked[right].dist
		}
		return ranked[left].name < ranked[right].name
	})
	if maxResults <= 0 || maxResults > len(ranked) {
		maxResults = len(ranked)
	}

	result := make([]string, 0, maxResults)
	for _, candidate := range ranked[:maxResults] {
		result = append(result, candidate.name)
	}
	return result
}

func levenshtein(left, right string) int {
	if left == right {
		return 0
	}
	leftRunes := []rune(left)
	rightRunes := []rune(right)
	if len(leftRunes) == 0 {
		return len(rightRunes)
	}
	if len(rightRunes) == 0 {
		return len(leftRunes)
	}

	previous := make([]int, len(rightRunes)+1)
	current := make([]int, len(rightRunes)+1)
	for index := range previous {
		previous[index] = index
	}
	for leftIndex := 1; leftIndex <= len(leftRunes); leftIndex++ {
		current[0] = leftIndex
		for rightIndex := 1; rightIndex <= len(rightRunes); rightIndex++ {
			cost := 1
			if leftRunes[leftIndex-1] == rightRunes[rightIndex-1] {
				cost = 0
			}
			current[rightIndex] = min(
				previous[rightIndex]+1,
				current[rightIndex-1]+1,
				previous[rightIndex-1]+cost,
			)
		}
		previous, current = current, previous
	}
	return previous[len(rightRunes)]
}

func editLimit(value string) int {
	if limit := len([]rune(value)) / 3; limit > 2 {
		return limit
	}
	return 2
}

func sharedPrefixLength(left, right string) int {
	leftRunes := []rune(left)
	rightRunes := []rune(right)
	length := 0
	for length < len(leftRunes) &&
		length < len(rightRunes) &&
		leftRunes[length] == rightRunes[length] {
		length++
	}
	return length
}
