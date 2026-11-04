resource "terraform_data" "redis" {
  input = "cache-redis"
}

output "id" {
  value = "cache"
}
