# Greets everyone.
class Greeter
  def greet(name)
    puts "Hello, #{name}!"
  end
end
Greeter.new.greet(:world)

TIMES = 2
puts "Tab:\tdone #{TIMES * 1.5}"
