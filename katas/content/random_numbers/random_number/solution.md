We can reuse the `RandomBit` operation from the ["Generate a single random bit"](random_bit) exercise.

We'll generate an N-bit random number by calling `RandomNBits` operation, where N is the bitsize of $max - min$. We can repeat this process until the result is less than or equal than $max - min$, and return that number plus $min$.

@[solution]({
    "id": "random_number_solution",
    "codePath": "solution.qs"
})
