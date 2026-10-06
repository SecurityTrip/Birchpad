# The demo bucket.
variable "region" {
  type    = string
  default = "eu-west-1"
}

resource "aws_s3_bucket" "demo" {
  bucket = "demo-${var.region}"
  tags = {
    Name    = "Demo"
    Enabled = true
  }
}

output "bucket" {
  value = upper(aws_s3_bucket.demo.id)
}

resource "aws_instance" "demo" {
  count         = 3
  instance_type = "t3.micro"

  lifecycle {
    create_before_destroy = true
  }
}
