//go:build fixture

package lib
import "testing"
func TestValue(t *testing.T) { if Value() != 42 { t.Fatal("value") } }
